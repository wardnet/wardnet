//! Background runner that drives `TunnelService::run_metrics_maintenance`.
//!
//! Same shape as [`crate::backup::runner::BackupCleanupRunner`] — owns a
//! cancellation token and a `JoinHandle`, exposes `start()` + `shutdown()`,
//! fires immediately on launch, then every `interval`. The job body is
//! idempotent: rolls up *any* complete day with no daily row yet, so a
//! daemon restart or missed run naturally catches up on the next tick.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::tunnel::service::TunnelService;

/// Default rollup cadence — hourly. The job body is cheap when there's
/// nothing to do (a single `SELECT` with a `WHERE NOT EXISTS`).
pub const DEFAULT_ROLLUP_INTERVAL: Duration = Duration::from_hours(1);

/// Background runner that wakes every `interval` to call
/// [`TunnelService::run_metrics_maintenance`].
pub struct TunnelMetricsRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl TunnelMetricsRunner {
    /// Start the runner. `parent` is the `wardnetd{version=...}` root span
    /// captured in `main.rs`.
    pub fn start(
        service: Arc<dyn TunnelService>,
        interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "tunnel_metrics_runner");
        let handle = tokio::spawn(runner_loop(service, interval, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Signal the runner to stop and await completion.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("tunnel metrics runner shut down");
    }
}

async fn runner_loop(
    service: Arc<dyn TunnelService>,
    interval: Duration,
    cancel: CancellationToken,
) {
    perform_tick(&service).await;

    let mut next = Instant::now() + interval;
    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("tunnel metrics runner cancellation received");
                break;
            }
            () = tokio::time::sleep_until(next) => {
                perform_tick(&service).await;
                next = Instant::now() + interval;
            }
        }
    }
}

async fn perform_tick(service: &Arc<dyn TunnelService>) {
    if let Err(e) = service.run_metrics_maintenance().await {
        tracing::warn!(
            error = %e,
            "tunnel metrics maintenance tick failed: {e}"
        );
    }
}
