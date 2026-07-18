//! Background runner that drains [`StatsBuffer`] into `stats_intraday` and
//! runs periodic maintenance (daily rollup + trim).
//!
//! - Every [`DEFAULT_FLUSH_INTERVAL`]: drain the buffer; if non-empty, call
//!   [`StatsService::run_flush`].
//! - Every [`DEFAULT_MAINTENANCE_INTERVAL`]: call [`StatsService::run_maintenance`].
//!   First run is deferred by [`STARTUP_MAINTENANCE_GRACE`] so other writers
//!   settle in before maintenance grabs the writer lock.

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use super::buffer::StatsBuffer;
use super::service::StatsService;
use crate::auth_context;
use crate::db_busy::retry_on_busy;

/// How often the buffer is drained and flushed to `stats_intraday`.
pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_secs(10);

/// How often rollup + trim maintenance runs.
pub const DEFAULT_MAINTENANCE_INTERVAL: Duration = Duration::from_hours(1);

/// Grace period before the *first* maintenance tick after the runner
/// starts. The DNS query-log runner and any pending API writes get this
/// long to settle before maintenance grabs the writer lock for its
/// rollup + trim. Decoupled from `flush_interval` so changing the
/// flush cadence doesn't quietly retune startup behaviour.
pub const STARTUP_MAINTENANCE_GRACE: Duration = Duration::from_secs(10);

/// How many times to retry `run_flush` on `SQLITE_BUSY`. Same shape
/// as the DNS query-log runner — two retries with a short backoff
/// covers the windows around `incremental_vacuum` / cleanup.
const FLUSH_BUSY_RETRIES: u32 = 2;
const FLUSH_BUSY_BACKOFF: Duration = Duration::from_millis(200);
const FLUSH_OPERATION: &str = "stats_flush";

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
            STARTUP_MAINTENANCE_GRACE,
            parent,
        )
    }

    /// Start the runner with custom intervals. Production callers use
    /// [`Self::start`]; tests pass shorter intervals or `Duration::ZERO`
    /// for `startup_maintenance_grace` to run maintenance immediately.
    pub fn start_with_intervals(
        buffer: Arc<StatsBuffer>,
        service: Arc<dyn StatsService>,
        flush_interval: Duration,
        maintenance_interval: Duration,
        startup_maintenance_grace: Duration,
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
                startup_maintenance_grace,
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
    startup_maintenance_grace: Duration,
    cancel: CancellationToken,
) {
    let mut flush_ticker = tokio::time::interval(flush_interval);
    flush_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    flush_ticker.tick().await; // skip the immediate tick — buffer is empty at startup

    // Defer the first maintenance run by `startup_maintenance_grace` so
    // other background writers (DNS query-log flush) get to settle in
    // before maintenance grabs the SQLite writer lock. Production uses
    // `STARTUP_MAINTENANCE_GRACE` (10 s); tests pass `Duration::ZERO`.
    let mut next_maintenance = Instant::now() + startup_maintenance_grace;

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
    // Retry the flush on transient writer-lock contention. `run_flush`
    // takes the rows by value, so each attempt clones them — fine,
    // batches are bounded and the clone is only paid on retry, which
    // should be rare under the 30 s `busy_timeout`.
    let result = retry_on_busy(
        FLUSH_OPERATION,
        FLUSH_BUSY_RETRIES,
        FLUSH_BUSY_BACKOFF,
        || {
            let rows = rows.clone();
            auth_context::with_context(
                AuthContext::Admin {
                    admin_id: Uuid::nil(),
                },
                service.run_flush(rows),
            )
        },
    )
    .await;
    if let Err(e) = result {
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
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    if let Err(e) = auth_context::with_context(admin_ctx, service.run_maintenance()).await {
        tracing::warn!(
            error = %e,
            "stats maintenance tick failed: {e}"
        );
    }
}
