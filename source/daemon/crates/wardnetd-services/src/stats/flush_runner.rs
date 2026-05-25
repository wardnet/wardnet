//! Background runner that drains [`StatsBuffer`] into `stats_intraday` and
//! runs periodic maintenance (daily rollup + trim).
//!
//! - Every [`DEFAULT_FLUSH_INTERVAL`]: drain the buffer; if non-empty, call
//!   [`StatsService::run_flush`].
//! - Every [`DEFAULT_MAINTENANCE_INTERVAL`]: call [`StatsService::run_maintenance`].
//!   Fires immediately on startup so a crash-restart doesn't miss a rollup.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use super::buffer::StatsBuffer;
use super::service::StatsService;

/// How often the buffer is drained and flushed to `stats_intraday`.
pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_secs(10);

/// How often rollup + trim maintenance runs.
pub const DEFAULT_MAINTENANCE_INTERVAL: Duration = Duration::from_hours(1);

/// Background runner for the stats subsystem.
pub struct StatsFlushRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl StatsFlushRunner {
    /// Start the runner with default intervals.
    pub fn start(
        buffer: Arc<StatsBuffer>,
        service: Arc<dyn StatsService>,
        parent: &tracing::Span,
    ) -> Self {
        Self::start_with_intervals(
            buffer,
            service,
            DEFAULT_FLUSH_INTERVAL,
            DEFAULT_MAINTENANCE_INTERVAL,
            parent,
        )
    }

    /// Start the runner with custom intervals. Production callers use
    /// [`Self::start`]; tests pass shorter intervals.
    pub fn start_with_intervals(
        buffer: Arc<StatsBuffer>,
        service: Arc<dyn StatsService>,
        flush_interval: Duration,
        maintenance_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "stats_flush_runner");
        let handle = tokio::spawn(
            runner_loop(
                buffer,
                service,
                flush_interval,
                maintenance_interval,
                cancel.clone(),
            )
            .instrument(span),
        );
        Self { cancel, handle }
    }

    /// Signal the runner to stop and await completion.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("stats flush runner shut down");
    }
}

async fn runner_loop(
    buffer: Arc<StatsBuffer>,
    service: Arc<dyn StatsService>,
    flush_interval: Duration,
    maintenance_interval: Duration,
    cancel: CancellationToken,
) {
    // Fire maintenance immediately on startup (idempotent rollup catches any
    // missed days from a previous crash).
    perform_maintenance(&service).await;

    let mut flush_ticker = tokio::time::interval(flush_interval);
    flush_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    flush_ticker.tick().await; // skip the immediate tick — buffer is empty at startup

    let mut next_maintenance = Instant::now() + maintenance_interval;

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("stats flush runner cancellation received");
                break;
            }
            _ = flush_ticker.tick() => {
                perform_flush(&buffer, &service).await;
            }
            () = tokio::time::sleep_until(next_maintenance) => {
                perform_maintenance(&service).await;
                next_maintenance = Instant::now() + maintenance_interval;
            }
        }
    }

    // Best-effort final flush on shutdown.
    perform_flush(&buffer, &service).await;
}

async fn perform_flush(buffer: &Arc<StatsBuffer>, service: &Arc<dyn StatsService>) {
    let rows = buffer.drain();
    if rows.is_empty() {
        return;
    }
    let count = rows.len();
    if let Err(e) = service.run_flush(rows).await {
        tracing::warn!(
            error = %e,
            count,
            "stats buffer flush failed: count={count}, error={e}"
        );
    } else {
        tracing::debug!(count, "stats buffer flushed: count={count}");
    }
}

async fn perform_maintenance(service: &Arc<dyn StatsService>) {
    if let Err(e) = service.run_maintenance().await {
        tracing::warn!(
            error = %e,
            "stats maintenance tick failed: {e}"
        );
    }
}
