//! Hard watchdog: ungated `/dev/watchdog` pet (issue #214).
//!
//! Pets the hardware watchdog on a fixed cadence **without ever consulting
//! health** — see the invariant in [`wardnetd_services::system::watchdog_ops`].
//! This is the backstop for a total runtime freeze, where even the health
//! refresh loop and the soft `sd_notify` ping can no longer run. If the whole
//! process wedges, the pets stop, and the kernel reboots the host within the
//! programmed hardware timeout.
//!
//! On clean shutdown the runner **disarms** first (magic-close), so a graceful
//! `systemctl stop` does not trigger a reboot.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use wardnetd_services::system::WatchdogOps;

/// Background task that keeps the hardware watchdog fed (issue #214).
pub struct HardwareWatchdogRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
    watchdog: Arc<dyn WatchdogOps>,
}

impl HardwareWatchdogRunner {
    /// Start petting `watchdog` every `pet_interval`. The interval must be
    /// comfortably below the device's programmed timeout (default 5 s pet vs
    /// 15 s timeout). A no-op-but-alive runner is returned even when the
    /// device is unavailable, so the shutdown sequence stays uniform.
    #[must_use]
    pub fn start(
        watchdog: Arc<dyn WatchdogOps>,
        pet_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "watchdog", layer = "hard");
        let handle = tokio::spawn(
            hard_loop(watchdog.clone(), pet_interval, cancel.clone()).instrument(span),
        );
        Self {
            cancel,
            handle,
            watchdog,
        }
    }

    /// Disarm the watchdog, then stop the pet loop.
    ///
    /// Disarm runs **first** so the device is in its no-reboot state for the
    /// remainder of the (possibly slow) shutdown sequence; a pet that races in
    /// before the loop notices the cancel is a harmless no-op on the
    /// now-closed device.
    pub async fn shutdown(self) {
        self.watchdog.disarm().await;
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("hardware watchdog runner shut down");
    }
}

async fn hard_loop(
    watchdog: Arc<dyn WatchdogOps>,
    pet_interval: Duration,
    cancel: CancellationToken,
) {
    if !watchdog.is_available() {
        // Nothing to feed — park on the cancel token so the runner still
        // shuts down cleanly. The daemon runs normally without the backstop.
        tracing::info!("hardware watchdog unavailable; pet loop idle");
        cancel.cancelled().await;
        return;
    }

    let mut ticker = interval(pet_interval);
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = ticker.tick() => {}
        }
        // UNGATED: never consult health here. This is the freeze backstop.
        watchdog.pet().await;
    }
}
