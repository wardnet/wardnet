//! Daily database-maintenance runner.
//!
//! Runs three maintenance operations once per calendar day, in order:
//!
//! 1. [`MaintenanceService::run_incremental_vacuum`] — return freed
//!    `SQLite` pages to the filesystem.
//! 2. [`MaintenanceService::run_wal_checkpoint`] — truncate the WAL
//!    sidecar back to ~0. Automatic checkpoints are always `PASSIVE` and
//!    never shrink the `-wal` file, so without this it parks at its
//!    high-water mark (observed at 530 MiB in the field) and drags every
//!    read and write.
//! 3. [`MaintenanceService::run_optimize`] — refresh the query planner's
//!    statistics (`ANALYZE` via `PRAGMA optimize`) so it keeps picking
//!    good indexes as tables grow.
//!
//! Fires independently of any domain-level feature flag so it benefits
//! **all** retention-driven tables (DNS query log, future event logs,
//! audit trails), not just the ones whose per-feature runner happens to be
//! active.
//!
//! Like every background component, it calls the auth-gated service under an
//! admin [`crate::auth_context`] rather than holding a repository directly.
//!
//! The daily cadence is enforced by checking the calendar date on every hourly
//! tick rather than sleeping for 24 hours, so the runner stays responsive to
//! cancellation without a 24-hour drain delay. The date of the last completed
//! sequence lives in the database, so a restart resumes the schedule rather
//! than restarting it.
//!
//! Every step reports at `info!`, including the ones with nothing to report.
//! A daily job whose only output is "something went wrong" cannot be told apart
//! from one that stopped running, and both look like a quiet journal — so the
//! successful, boring case is exactly the one that has to leave a trace.
//! `logging.journal_info_targets` carries this module so those lines reach
//! `journalctl -u wardnetd` and not just the rotating log file.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::maintenance::MaintenanceService;

/// How often the runner wakes to check whether a day has rolled over.
pub const TICK_INTERVAL: Duration = Duration::from_hours(1);

/// Background task that runs [`MaintenanceRepository::incremental_vacuum`]
/// once per calendar day.
pub struct DbMaintenanceRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DbMaintenanceRunner {
    /// Start with the default hourly tick. Production entry point.
    pub fn start(maintenance: Arc<dyn MaintenanceService>, parent: &tracing::Span) -> Self {
        Self::start_with_interval(maintenance, TICK_INTERVAL, parent)
    }

    /// Start with a custom tick interval. Tests pass short intervals so they
    /// can exercise the day-rollover logic without sleeping for an hour.
    pub fn start_with_interval(
        maintenance: Arc<dyn MaintenanceService>,
        tick_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "db_maintenance_runner");

        let handle =
            tokio::spawn(runner_loop(maintenance, tick_interval, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the runner and wait for it to stop.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DB maintenance runner shut down");
    }
}

async fn runner_loop(
    maintenance: Arc<dyn MaintenanceService>,
    tick_interval: Duration,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::system();

    let mut ticker = tokio::time::interval(tick_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await; // skip the immediate first tick

    // Resume the schedule from the database rather than assuming this boot
    // starts a fresh day. Seeding with "today" instead meant a daemon that
    // restarted before its next UTC rollover — for an update, a reboot, a
    // watchdog SIGABRT — pushed the run out another full day, indefinitely
    // for a box that restarts daily. A read failure reads as "never ran",
    // which costs one extra sequence and no correctness.
    let mut last_run_day =
        match auth_context::with_context(admin_ctx.clone(), maintenance.last_maintenance_day())
            .await
        {
            Ok(day) => day,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "could not read last maintenance day; treating as never run: {e}"
                );
                None
            }
        };

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DB maintenance runner cancellation received");
                break;
            }
            _ = ticker.tick() => {
                let today_now = today();
                if last_run_day != Some(today_now) {
                    last_run_day = Some(today_now);
                    run_daily_maintenance(maintenance.as_ref(), &admin_ctx).await;
                    record_day(maintenance.as_ref(), &admin_ctx, today_now).await;
                }
            }
        }
    }
}

/// Persist the day the sequence just ran on.
///
/// Written after the sequence, not before: a run that dies partway through
/// leaves the date unwritten, so the next tick retries rather than counting a
/// half-finished sequence as done. The in-memory marker is advanced either way,
/// so a persistently failing write costs one retry per restart, not a loop.
async fn record_day(
    maintenance: &dyn MaintenanceService,
    admin_ctx: &AuthContext,
    day: chrono::NaiveDate,
) {
    if let Err(e) =
        auth_context::with_context(admin_ctx.clone(), maintenance.record_maintenance_day(day)).await
    {
        tracing::warn!(error = %e, "failed to record maintenance day: {e}");
    }
}

/// Run the full daily maintenance sequence: vacuum, WAL checkpoint,
/// optimize. Each step is independent — a failure in one is logged and
/// the next still runs, so a busy checkpoint never skips the planner
/// refresh. The order matters: vacuum first moves freed pages onto the
/// freelist (writing WAL frames), then the checkpoint folds them in and
/// truncates the sidecar, then optimize refreshes statistics.
pub(crate) async fn run_daily_maintenance(
    maintenance: &dyn MaintenanceService,
    admin_ctx: &AuthContext,
) {
    run_vacuum(maintenance, admin_ctx).await;
    run_checkpoint(maintenance, admin_ctx).await;
    run_optimize(maintenance, admin_ctx).await;
}

pub(crate) async fn run_vacuum(maintenance: &dyn MaintenanceService, admin_ctx: &AuthContext) {
    match auth_context::with_context(admin_ctx.clone(), maintenance.run_incremental_vacuum()).await
    {
        Ok(outcome) => {
            // Logged unconditionally, reclaimed pages or not. "Reclaimed 0"
            // and "did not run" are the same silence otherwise, and they call
            // for opposite responses: the first is a database with nothing to
            // give back, the second is one that has quietly stopped shrinking.
            // `stop` says which — `stalled` with a large `freelist_after` is
            // the one to act on.
            //
            // Pages rather than bytes: the page size is a property of the file,
            // not of this run, and multiplying here would bake a guess at it
            // into every line. `freelist_after` × `page_size` is the reclaimable
            // remainder when someone needs the figure in bytes.
            tracing::info!(
                reclaimed_pages = outcome.reclaimed_pages,
                freelist_before = outcome.freelist_before,
                freelist_after = outcome.freelist_after,
                page_count = outcome.page_count_after,
                chunks = outcome.chunks,
                // Display rather than `as_str()`: the message text below
                // renders the same value through `Display` anyway, and the two
                // agree by construction (`Display` forwards to `as_str`), so
                // going through it once keeps the field and the text from ever
                // disagreeing about what `stop` was.
                stop = %outcome.stop,
                "incremental vacuum finished: reclaimed_pages={reclaimed_pages}, \
                 freelist_before={freelist_before}, freelist_after={freelist_after}, \
                 page_count={page_count}, chunks={chunks}, stop={stop}",
                reclaimed_pages = outcome.reclaimed_pages,
                freelist_before = outcome.freelist_before,
                freelist_after = outcome.freelist_after,
                page_count = outcome.page_count_after,
                chunks = outcome.chunks,
                stop = outcome.stop,
            );
        }
        Err(e) => {
            tracing::warn!(error = %e, "incremental vacuum failed: {e}");
        }
    }
}

pub(crate) async fn run_checkpoint(maintenance: &dyn MaintenanceService, admin_ctx: &AuthContext) {
    match auth_context::with_context(admin_ctx.clone(), maintenance.run_wal_checkpoint()).await {
        Ok(outcome) if outcome.busy => {
            // A reader held a snapshot, so the WAL couldn't be truncated
            // this pass. Not an error — the next daily tick retries.
            tracing::info!(
                wal_frames = outcome.wal_frames,
                checkpointed_frames = outcome.checkpointed_frames,
                "WAL checkpoint could not truncate (reader active); will retry next tick"
            );
        }
        Ok(outcome) => {
            tracing::info!(
                wal_frames = outcome.wal_frames,
                checkpointed_frames = outcome.checkpointed_frames,
                "WAL checkpoint truncated sidecar: checkpointed_frames={checkpointed_frames}",
                checkpointed_frames = outcome.checkpointed_frames,
            );
        }
        Err(e) => {
            tracing::warn!(error = %e, "WAL checkpoint failed: {e}");
        }
    }
}

pub(crate) async fn run_optimize(maintenance: &dyn MaintenanceService, admin_ctx: &AuthContext) {
    match auth_context::with_context(admin_ctx.clone(), maintenance.run_optimize()).await {
        Ok(()) => {
            // Logged at info! to match the vacuum and checkpoint steps so
            // the full daily maintenance cycle is visible at one log level.
            tracing::info!("database optimize (ANALYZE via PRAGMA optimize) complete");
        }
        Err(e) => {
            tracing::warn!(error = %e, "database optimize failed: {e}");
        }
    }
}

fn today() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive()
}
