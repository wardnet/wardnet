//! Hardware watchdog operations (issue #214).
//!
//! Behind a trait so the production daemon can drive the Linux
//! `/dev/watchdog` character device while the mock daemon and unit tests plug
//! in a logging no-op that never touches a real device (and never reboots the
//! developer's machine).
//!
//! ## The ungated invariant
//!
//! Unlike every other recovery hook, the hardware watchdog is pet **without
//! consulting health**. It is the last-resort backstop for a *total* runtime
//! freeze — the case where even the health-refresh loop and the soft
//! `sd_notify` ping can no longer run (e.g. the process is in uninterruptible
//! D-state). Gating the pet on health would defeat its only purpose. The
//! proportionate, health-aware recovery lives one layer up in the soft
//! watchdog; this layer exists solely so that "nothing is running at all"
//! still ends in a kernel reboot within the hardware timeout.

use async_trait::async_trait;

/// Drives the platform hardware watchdog.
///
/// Wired onto [`crate::Backends`] like [`crate::system::SystemPowerOps`] and
/// [`crate::garp::GarpOps`]. Methods take `&self` and don't return `Result`:
/// a watchdog write failure is logged inside the implementation and otherwise
/// swallowed, because there is no useful caller-side recovery — the runner
/// simply pets again on the next tick.
#[async_trait]
pub trait WatchdogOps: Send + Sync {
    /// Pet (keep-alive) the watchdog so the kernel does not reboot the host.
    /// Called on a fixed cadence by `HardwareWatchdogRunner`, **ungated** by
    /// health. A no-op when the device is unavailable.
    async fn pet(&self);

    /// Disarm the watchdog so a subsequent clean shutdown does **not** trigger
    /// a reboot. The Linux implementation writes the magic `'V'` character and
    /// closes the device (the kernel "magic close" contract). Called first in
    /// the daemon's shutdown sequence.
    async fn disarm(&self);

    /// Whether a usable watchdog device was opened. `false` on boards without
    /// `/dev/watchdog`, in the mock, and after a failed open — in which case
    /// `pet`/`disarm` are no-ops and the daemon runs normally without the
    /// hardware backstop.
    #[must_use]
    fn is_available(&self) -> bool;
}
