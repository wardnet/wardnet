use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use wardnetd_services::HealthMonitor;

/// Default cadence between [`HealthMonitor::refresh`] cycles. Overridable via
/// `health.refresh_interval_secs`. Five seconds keeps the snapshot well within
/// the soft watchdog's `2 × interval` staleness window and the systemd
/// `WatchdogSec=15` budget.
pub const DEFAULT_HEALTH_INTERVAL: Duration = Duration::from_secs(5);

/// Background task that re-runs every registered [`HealthCheck`] on a fixed
/// tick (issue #214).
///
/// Modeled on [`crate::heartbeat::HeartbeatRunner`]: logs land under a child
/// span named `health` (see `.agents/observability.md`), and a
/// [`CancellationToken`] drives a clean shutdown. Failures never escalate —
/// `refresh()` swallows per-check errors into the snapshot, so the loop itself
/// cannot fail.
pub struct HealthMonitorRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl HealthMonitorRunner {
    /// Start the runner with the default interval.
    #[must_use]
    pub fn start(monitor: Arc<HealthMonitor>, parent: &tracing::Span) -> Self {
        Self::start_with_interval(monitor, DEFAULT_HEALTH_INTERVAL, parent)
    }

    /// Start with a custom interval — used by tests to drive the loop faster.
    #[must_use]
    pub fn start_with_interval(
        monitor: Arc<HealthMonitor>,
        tick: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "health");
        let handle = tokio::spawn(health_loop(monitor, tick, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the loop and wait for the task to exit.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("health monitor runner shut down");
    }
}

async fn health_loop(monitor: Arc<HealthMonitor>, tick: Duration, cancel: CancellationToken) {
    // `interval` fires immediately on the first `tick()`, so the first refresh
    // happens right away — but startup also calls `refresh()` once before
    // signalling readiness, so the first published snapshot already reflects
    // real checks by the time anything reads it.
    let mut ticker = interval(tick);
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = ticker.tick() => {}
        }
        monitor.refresh().await;
    }
}
