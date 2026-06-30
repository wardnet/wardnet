//! Soft watchdog: health-gated `sd_notify(WATCHDOG=1)` (issue #214).
//!
//! systemd is told `Type=notify` + `WatchdogSec=15`. This runner pings
//! `WATCHDOG=1` on a cadence of `WATCHDOG_USEC/2` **only while** overall
//! health is UP and the snapshot is fresh. If health goes DOWN — or the
//! refresh loop itself stalls, making the snapshot stale — the ping is
//! withheld, the 15 s budget elapses, and systemd restarts the *service*
//! (seconds; the Pi stays up). This is the proportionate middle layer between
//! "report status" and "reboot the host".
//!
//! The ping is gated; the hardware `/dev/watchdog` pet in [`super::hard`] is
//! not. Only this layer consults health.

use std::sync::Arc;
use std::time::Duration;

use sd_notify::NotifyState;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use wardnetd_services::{HealthMonitor, HealthSnapshot, HealthStatus};

/// The gating decision: pet systemd's watchdog only when overall health is UP
/// **and** the snapshot is fresh (the refresh loop is still running). Extracted
/// so the policy is unit-testable without driving the background loop.
#[must_use]
pub(crate) fn should_ping(snapshot: &HealthSnapshot, staleness_max_age: Duration) -> bool {
    snapshot.overall == HealthStatus::Up && snapshot.is_fresh(staleness_max_age)
}

/// Abstraction over the two `sd_notify` signals this subsystem sends, so tests
/// can inject a fake that records `WATCHDOG=1` pings instead of talking to a
/// real `NOTIFY_SOCKET`.
pub trait Notifier: Send + Sync {
    /// Send `READY=1`. Called once by `main.rs` after all listeners bind, so
    /// systemd only considers the `Type=notify` service started once it can
    /// actually serve.
    fn notify_ready(&self);

    /// Send `WATCHDOG=1`. Called by the runner on each tick **only** when
    /// healthy + fresh.
    fn ping_watchdog(&self);
}

/// Production [`Notifier`] backed by the `sd-notify` crate.
///
/// When `NOTIFY_SOCKET` is unset (dev runs, the mock), `sd_notify::notify`
/// returns `Ok(())` without doing anything — so both methods are graceful
/// no-ops outside systemd.
#[derive(Debug, Default, Clone)]
pub struct SdNotifier;

impl SdNotifier {
    /// The systemd-recommended ping interval — `WATCHDOG_USEC / 2` — when the
    /// service was started with `WatchdogSec`. `None` when not supervised by a
    /// watchdog (e.g. `WatchdogSec` unset, or running outside systemd), in
    /// which case the caller falls back to a config-derived interval.
    #[must_use]
    pub fn recommended_interval() -> Option<Duration> {
        // sd-notify 0.5 returns the configured `WATCHDOG_USEC` as a `Duration`
        // (or `None` when unsupervised), replacing the 0.4 out-param form.
        sd_notify::watchdog_enabled()
            .filter(|d| !d.is_zero())
            .map(|d| d / 2)
    }
}

impl Notifier for SdNotifier {
    fn notify_ready(&self) {
        match sd_notify::notify(&[NotifyState::Ready]) {
            Ok(()) => tracing::info!("sd_notify READY=1 sent (listeners bound)"),
            Err(e) => tracing::warn!(error = %e, "sd_notify READY=1 failed: {e}"),
        }
    }

    fn ping_watchdog(&self) {
        if let Err(e) = sd_notify::notify(&[NotifyState::Watchdog]) {
            tracing::warn!(error = %e, "sd_notify WATCHDOG=1 failed: {e}");
        }
    }
}

/// Background task that health-gates the systemd watchdog ping (issue #214).
pub struct SoftWatchdogRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl SoftWatchdogRunner {
    /// Start the runner.
    ///
    /// * `tick` — how often to evaluate health and (maybe) ping. Should be
    ///   `WATCHDOG_USEC/2`.
    /// * `staleness_max_age` — a snapshot older than this is treated as
    ///   "refresh loop stalled" ⇒ withhold. Use `2 × health refresh interval`.
    #[must_use]
    pub fn start(
        monitor: Arc<HealthMonitor>,
        notifier: Arc<dyn Notifier>,
        tick: Duration,
        staleness_max_age: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "watchdog", layer = "soft");
        let handle = tokio::spawn(
            soft_loop(monitor, notifier, tick, staleness_max_age, cancel.clone()).instrument(span),
        );
        Self { cancel, handle }
    }

    /// Cancel the loop and wait for the task to exit.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("soft watchdog runner shut down");
    }
}

async fn soft_loop(
    monitor: Arc<HealthMonitor>,
    notifier: Arc<dyn Notifier>,
    tick: Duration,
    staleness_max_age: Duration,
    cancel: CancellationToken,
) {
    let mut ticker = interval(tick);
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = ticker.tick() => {}
        }

        let snapshot = monitor.snapshot();

        if should_ping(&snapshot, staleness_max_age) {
            notifier.ping_watchdog();
        } else {
            // Withholding the ping is the whole point — let WatchdogSec
            // elapse so systemd restarts the service. WARN so the cause is
            // visible in the journal right up to the restart.
            tracing::warn!(
                overall = ?snapshot.overall,
                fresh = snapshot.is_fresh(staleness_max_age),
                "withholding sd_notify WATCHDOG=1 (unhealthy or stale); \
                 systemd will restart the service if this persists"
            );
        }
    }
}
