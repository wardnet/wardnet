//! Background runner that drains the DNS log sink into `SQLite`, applies
//! retention cleanup once a day, and surfaces dropped-entry counters.
//!
//! Lifecycle:
//! - Starts when the daemon boots; stops on cancellation token.
//! - Drains up to [`BATCH_MAX`] entries OR every [`BATCH_INTERVAL`] tick,
//!   whichever fires first, into a single transactional insert.
//! - Once per day (configurable in tests via the constructor), runs
//!   `DnsRepository::cleanup_query_log(retention_days)` to enforce
//!   retention. VACUUM is intentionally out of scope.
//! - Honors `query_log_enabled = false` by draining the channel into
//!   the void: still consumes (so the producer never blocks) but skips
//!   the insert. Broadcast streaming is independent and untouched.
//! - If [`DnsLogSink::take_dropped`] is non-zero, logs at `warn!` no
//!   more than once per [`DROPPED_REPORT_INTERVAL`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::DnsService;
use crate::auth_context;
use crate::dns::log_sink::DnsLogSink;
use wardnetd_data::repository::{DnsRepository, QueryLogRow};

/// Maximum entries flushed in a single insert batch.
pub const BATCH_MAX: usize = 256;
/// Maximum time between flushes when the buffer never reaches `BATCH_MAX`.
pub const BATCH_INTERVAL: Duration = Duration::from_secs(1);
/// Daily cleanup tick interval (parent ticker, day-rollover gated inside).
pub const CLEANUP_TICK_INTERVAL: Duration = Duration::from_hours(1);
/// Minimum interval between dropped-counter warnings.
pub const DROPPED_REPORT_INTERVAL: Duration = Duration::from_mins(1);

/// Background runner draining the persistence side of [`DnsLogSink`].
pub struct DnsQueryLogRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DnsQueryLogRunner {
    /// Start the runner. The receiver is taken once at startup; the sink
    /// is kept around so the runner can drain its dropped-counter.
    pub fn start(
        service: Arc<dyn DnsService>,
        dns_repo: Arc<dyn DnsRepository>,
        sink: Arc<DnsLogSink>,
        rx: mpsc::Receiver<QueryLogRow>,
        parent: &tracing::Span,
    ) -> Self {
        Self::start_with_intervals(
            service,
            dns_repo,
            sink,
            rx,
            BATCH_INTERVAL,
            CLEANUP_TICK_INTERVAL,
            parent,
        )
    }

    /// Start the runner with custom flush + cleanup tick intervals.
    /// Production callers use [`Self::start`]; tests pass shorter
    /// intervals to exercise the loop without waiting.
    pub fn start_with_intervals(
        service: Arc<dyn DnsService>,
        dns_repo: Arc<dyn DnsRepository>,
        sink: Arc<DnsLogSink>,
        rx: mpsc::Receiver<QueryLogRow>,
        batch_interval: Duration,
        cleanup_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dns_query_log_runner");

        let handle = tokio::spawn(
            runner_loop(
                service,
                dns_repo,
                sink,
                rx,
                batch_interval,
                cleanup_interval,
                cancel.clone(),
            )
            .instrument(span),
        );

        Self { cancel, handle }
    }

    /// Cancel and join.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DNS query log runner shut down");
    }
}

#[allow(clippy::too_many_arguments)]
async fn runner_loop(
    service: Arc<dyn DnsService>,
    dns_repo: Arc<dyn DnsRepository>,
    sink: Arc<DnsLogSink>,
    mut rx: mpsc::Receiver<QueryLogRow>,
    batch_interval: Duration,
    cleanup_interval: Duration,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

    let mut buffer: Vec<QueryLogRow> = Vec::with_capacity(BATCH_MAX);
    let mut flush_ticker = tokio::time::interval(batch_interval);
    flush_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    flush_ticker.tick().await; // skip the immediate first tick

    let mut cleanup_ticker = tokio::time::interval(cleanup_interval);
    cleanup_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    cleanup_ticker.tick().await;

    let mut last_cleanup_day = today();
    let mut last_dropped_report = Instant::now();

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DNS query log runner cancellation received");
                break;
            }
            recv = rx.recv() => {
                let Some(row) = recv else {
                    tracing::info!("DNS log sink closed; exiting runner");
                    break;
                };
                buffer.push(row);
                if buffer.len() >= BATCH_MAX {
                    flush(&service, dns_repo.as_ref(), &admin_ctx, &mut buffer).await;
                }
            }
            _ = flush_ticker.tick() => {
                if !buffer.is_empty() {
                    flush(&service, dns_repo.as_ref(), &admin_ctx, &mut buffer).await;
                }
                if last_dropped_report.elapsed() >= DROPPED_REPORT_INTERVAL {
                    let dropped = sink.take_dropped();
                    if dropped > 0 {
                        tracing::warn!(
                            dropped,
                            "DNS query log dropped entries (channel full): dropped={dropped}"
                        );
                    }
                    last_dropped_report = Instant::now();
                }
            }
            _ = cleanup_ticker.tick() => {
                let today_now = today();
                if today_now != last_cleanup_day {
                    last_cleanup_day = today_now;
                    cleanup(&service, dns_repo.as_ref(), &admin_ctx).await;
                }
            }
        }
    }

    // Drain remaining entries on shutdown — best effort.
    if !buffer.is_empty() {
        flush(&service, dns_repo.as_ref(), &admin_ctx, &mut buffer).await;
    }
}

async fn flush(
    service: &Arc<dyn DnsService>,
    dns_repo: &dyn DnsRepository,
    admin_ctx: &AuthContext,
    buffer: &mut Vec<QueryLogRow>,
) {
    if buffer.is_empty() {
        return;
    }

    let enabled =
        match auth_context::with_context(admin_ctx.clone(), service.get_dns_config()).await {
            Ok(cfg) => cfg.query_log_enabled,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to read DNS config; treating query_log as enabled: error={e}"
                );
                true
            }
        };

    if !enabled {
        // Drain into the void — still consume to avoid backpressure.
        let dropped = buffer.len();
        buffer.clear();
        tracing::debug!(
            dropped,
            "query log persistence disabled, drained without insert: dropped={dropped}"
        );
        return;
    }

    let count = buffer.len();
    let started = Instant::now();
    let result = dns_repo.insert_query_log_batch(buffer).await;
    buffer.clear();

    match result {
        Ok(()) => {
            tracing::debug!(
                count,
                elapsed_ms = started.elapsed().as_millis(),
                "flushed DNS query log batch: count={count}"
            );
        }
        Err(e) => {
            tracing::error!(
                error = %e,
                count,
                "failed to flush DNS query log batch: count={count}, error={e}"
            );
        }
    }
}

async fn cleanup(
    service: &Arc<dyn DnsService>,
    dns_repo: &dyn DnsRepository,
    admin_ctx: &AuthContext,
) {
    let cfg = match auth_context::with_context(admin_ctx.clone(), service.get_dns_config()).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "skip cleanup: failed to load DNS config: {e}");
            return;
        }
    };

    if !cfg.query_log_enabled {
        tracing::debug!("query log disabled; skipping cleanup tick");
        return;
    }

    let retention = cfg.query_log_retention_days.max(1);
    match dns_repo.cleanup_query_log(retention).await {
        Ok(deleted) => {
            tracing::info!(
                deleted,
                retention_days = retention,
                "DNS query log cleanup: deleted={deleted}, retention_days={retention_days}",
                deleted = deleted,
                retention_days = retention,
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "DNS query log cleanup failed: {e}");
        }
    }
}

fn today() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive()
}
