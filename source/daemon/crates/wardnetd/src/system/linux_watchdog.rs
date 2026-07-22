//! Linux `/dev/watchdog` implementation of [`WatchdogOps`] (issue #214).
//!
//! Opens the hardware watchdog character device `O_WRONLY`, programs its
//! timeout via the `WDIOC_SETTIMEOUT` ioctl, and keeps it alive by writing a
//! byte on each pet. On clean shutdown it performs the kernel **magic close**
//! (write `'V'`, then close the fd) so a graceful stop does not reboot the
//! host.
//!
//! If the device is absent (`ENOENT` — most dev boxes, and Pis without a
//! watchdog driver loaded), the constructor logs `"watchdog unavailable,
//! skipping"` and returns an instance whose [`is_available`] is `false`;
//! every operation is then a no-op and the daemon runs normally without the
//! hardware backstop.
//!
//! [`is_available`]: WatchdogOps::is_available

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use wardnetd_services::system::WatchdogOps;

/// `'W'` — the watchdog ioctl type, from `linux/watchdog.h`.
#[allow(clippy::cast_lossless)] // `u32::from` isn't const-stable; this is a const.
const WATCHDOG_IOCTL_BASE: u32 = b'W' as u32;
/// `_IOC_WRITE` direction bit.
const IOC_WRITE: u32 = 1;
/// `_IOC_READ` direction bit.
const IOC_READ: u32 = 2;
/// Size, in bytes, of the `int` argument these ioctls carry.
const INT_SIZE: u32 = 4;

/// Reconstruct the kernel `_IOC(dir, type, nr, size)` request value. Mirrors
/// the `asm-generic/ioctl.h` bit layout so we can name the constants locally
/// rather than pull in `nix` just for two ioctls.
const fn ioc(dir: u32, typ: u32, nr: u32, size: u32) -> u32 {
    (dir << 30) | (size << 16) | (typ << 8) | nr
}

/// `WDIOC_SETTIMEOUT` = `_IOWR(WATCHDOG_IOCTL_BASE, 6, int)` — program the
/// hardware reboot timeout, in seconds.
const WDIOC_SETTIMEOUT: u32 = ioc(IOC_READ | IOC_WRITE, WATCHDOG_IOCTL_BASE, 6, INT_SIZE);

/// Hardware-watchdog operations backed by the Linux `/dev/watchdog` device.
///
/// Holds the open device behind an `Arc<Mutex>` so pets (which write a byte)
/// and the one-shot disarm (which takes the file and closes it) are serialised
/// — and so the blocking write/close syscalls can be moved off the async
/// executor via `spawn_blocking` (the `Arc` is cloned into the blocking task).
/// `available` mirrors whether the device is currently open, so the sync
/// [`WatchdogOps::is_available`] needs no lock.
pub struct LinuxWatchdog {
    device: Arc<Mutex<Option<std::fs::File>>>,
    available: AtomicBool,
    path: PathBuf,
}

impl LinuxWatchdog {
    /// Open and arm the device, programming `timeout_secs` as the reboot
    /// window. Never fails: an unopenable device yields an unavailable
    /// instance (no-op pets/disarm) so the daemon still boots.
    #[must_use]
    pub fn open(path: PathBuf, timeout_secs: u64) -> Self {
        match OpenOptions::new().write(true).open(&path) {
            Ok(file) => {
                Self::set_timeout(&file, timeout_secs, &path);
                Self {
                    device: Arc::new(Mutex::new(Some(file))),
                    available: AtomicBool::new(true),
                    path,
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // No watchdog device at all: a VM/container without one, or a
                // host whose watchdog driver isn't loaded. Expected, not a
                // fault — the soft (sd_notify) layer still supervises.
                tracing::info!(
                    path = %path.display(),
                    "no hardware watchdog device present; running with the soft watchdog only ({})",
                    path.display(),
                );
                Self::unavailable(path)
            }
            // Match `ResourceBusy` by kind, but also fall back to the raw
            // `EBUSY` (16) so a std build that doesn't map the errno to that
            // kind still classifies it here rather than dropping through to the
            // alarming "no backstop" arm below.
            Err(e)
                if e.kind() == std::io::ErrorKind::ResourceBusy || e.raw_os_error() == Some(16) =>
            {
                // EBUSY: another supervisor already holds the single-opener
                // device and is petting it — e.g. systemd's own
                // `RuntimeWatchdogSec=` (the Raspberry Pi OS default), or a
                // standalone `watchdogd`. The host therefore still HAS a
                // hardware reboot-on-freeze backstop; it's simply owned by
                // that supervisor rather than wardnet. Our hard layer stands
                // down (the soft layer keeps restarting the service on a
                // daemon-specific hang), so this is INFO, not a fault.
                tracing::info!(
                    path = %path.display(),
                    "hardware watchdog is owned by another supervisor (e.g. systemd RuntimeWatchdogSec); the host is still covered — wardnet's hard layer will stay idle",
                );
                Self::unavailable(path)
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                // EACCES: the device exists and is free, but this unprivileged
                // process can't open it. This is a real, fixable
                // misconfiguration — the daemon runs without a hardware
                // backstop until the wardnet user can access the device.
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "cannot open watchdog device (permission denied); running without a hardware backstop — ensure the udev rule granting the wardnet group access to {} is installed (post-upgrade migration 0002_watchdog_udev_rule)",
                    path.display(),
                );
                Self::unavailable(path)
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %path.display(),
                    "failed to open watchdog device; running without hardware backstop: {e}",
                );
                Self::unavailable(path)
            }
        }
    }

    /// An explicitly-disabled instance (config `watchdog.enabled = false`).
    /// Never opens a device; every op is a no-op and [`is_available`] is
    /// `false`.
    ///
    /// [`is_available`]: WatchdogOps::is_available
    #[must_use]
    pub fn disabled() -> Self {
        Self::unavailable(PathBuf::from("<disabled>"))
    }

    fn unavailable(path: PathBuf) -> Self {
        Self {
            device: Arc::new(Mutex::new(None)),
            available: AtomicBool::new(false),
            path,
        }
    }

    fn set_timeout(file: &std::fs::File, timeout_secs: u64, path: &std::path::Path) {
        let mut timeout: libc::c_int = i32::try_from(timeout_secs).unwrap_or(15);
        // SAFETY: `file` owns a valid fd for the duration of this call;
        // `WDIOC_SETTIMEOUT` reads/writes a single `c_int` through the
        // provided pointer, which is a live local. No aliasing or lifetime
        // concerns.
        let rc = unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                libc::c_ulong::from(WDIOC_SETTIMEOUT),
                &mut timeout,
            )
        };
        if rc == 0 {
            tracing::info!(
                path = %path.display(),
                timeout_secs = timeout,
                "hardware watchdog armed: path={}, timeout={}s",
                path.display(),
                timeout,
            );
        } else {
            tracing::warn!(
                error = %std::io::Error::last_os_error(),
                path = %path.display(),
                "WDIOC_SETTIMEOUT failed; relying on the device's default timeout",
            );
        }
    }
}

#[async_trait]
impl WatchdogOps for LinuxWatchdog {
    async fn pet(&self) {
        // The write to `/dev/watchdog` is a blocking syscall; run it on the
        // blocking pool so it can never park an async executor thread, even if
        // the device driver stalls.
        let device = self.device.clone();
        let path = self.path.clone();
        let _ = tokio::task::spawn_blocking(move || {
            let mut guard = device
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(file) = guard.as_mut() {
                // Any byte other than 'V' is a keep-alive. A 0-byte is the
                // conventional choice and can never be mistaken for the
                // magic-close sentinel.
                if let Err(e) = file.write_all(b"\0") {
                    tracing::warn!(error = %e, path = %path.display(), "watchdog pet failed: {e}");
                }
            }
        })
        .await;
    }

    async fn disarm(&self) {
        let device = self.device.clone();
        let disarmed = tokio::task::spawn_blocking(move || {
            let mut guard = device
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(mut file) = guard.take() {
                // Magic close: writing 'V' before the fd closes tells the
                // kernel to disable the watchdog rather than reboot. Dropping
                // `file` performs the (blocking) close.
                if let Err(e) = file.write_all(b"V") {
                    tracing::warn!(error = %e, "watchdog disarm ('V') write failed; closing anyway: {e}");
                }
                drop(file);
                true
            } else {
                false
            }
        })
        .await
        .unwrap_or(false);

        if disarmed {
            self.available.store(false, Ordering::SeqCst);
            tracing::info!(path = %self.path.display(), "hardware watchdog disarmed (magic close)");
        }
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::SeqCst)
    }
}
