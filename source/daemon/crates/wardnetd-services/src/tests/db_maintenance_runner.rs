use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::db_maintenance_runner::{
    DbMaintenanceRunner, run_checkpoint, run_daily_maintenance, run_vacuum,
};
use crate::error::AppError;
use crate::maintenance::MaintenanceService;
use wardnetd_data::repository::{IncrementalVacuumOutcome, VacuumStop, WalCheckpointOutcome};

// ── Mock maintenance service ──────────────────────────────────────────────────

struct MockMaintenance {
    result: anyhow::Result<IncrementalVacuumOutcome>,
    /// When the service is `ok`, whether the checkpoint reports `busy`
    /// (a reader blocked the truncation) rather than a clean truncate.
    checkpoint_busy: bool,
    /// Calls to `run_incremental_vacuum`.
    calls: Mutex<u32>,
    /// Calls to `run_wal_checkpoint`.
    checkpoint_calls: Mutex<u32>,
    /// Calls to `run_optimize`.
    optimize_calls: Mutex<u32>,
    /// The persisted last-run day the runner reads at startup.
    last_day: Mutex<Option<chrono::NaiveDate>>,
    /// Days handed to `record_maintenance_day`, in call order.
    recorded_days: Mutex<Vec<chrono::NaiveDate>>,
}

/// An outcome that reclaimed `pages` and drained the freelist.
fn drained(pages: u64) -> IncrementalVacuumOutcome {
    IncrementalVacuumOutcome {
        reclaimed_pages: pages,
        freelist_before: i64::try_from(pages).unwrap(),
        freelist_after: 0,
        page_count_after: 1_000,
        chunks: u32::from(pages > 0),
        stop: VacuumStop::Drained,
    }
}

impl MockMaintenance {
    fn ok(reclaimed: u64) -> Arc<Self> {
        Self::with(Ok(drained(reclaimed)), false)
    }

    /// Like [`ok`](Self::ok) but the checkpoint reports `busy` so the
    /// runner's reader-active log branch is exercised.
    fn ok_busy_checkpoint() -> Arc<Self> {
        Self::with(Ok(drained(0)), true)
    }

    fn err() -> Arc<Self> {
        Self::with(Err(anyhow::anyhow!("synthetic vacuum error")), false)
    }

    fn with(result: anyhow::Result<IncrementalVacuumOutcome>, checkpoint_busy: bool) -> Arc<Self> {
        Arc::new(Self {
            result,
            checkpoint_busy,
            calls: Mutex::new(0),
            checkpoint_calls: Mutex::new(0),
            optimize_calls: Mutex::new(0),
            // Never run: a fresh database, so the first tick fires.
            last_day: Mutex::new(None),
            recorded_days: Mutex::new(Vec::new()),
        })
    }

    /// Seed the persisted last-run day, standing in for a database that has
    /// already seen a maintenance sequence on `day`.
    fn ran_on(self: &Arc<Self>, day: chrono::NaiveDate) {
        *self.last_day.lock().unwrap() = Some(day);
    }

    fn call_count(&self) -> u32 {
        *self.calls.lock().unwrap()
    }

    fn checkpoint_count(&self) -> u32 {
        *self.checkpoint_calls.lock().unwrap()
    }

    fn optimize_count(&self) -> u32 {
        *self.optimize_calls.lock().unwrap()
    }

    fn recorded(&self) -> Vec<chrono::NaiveDate> {
        self.recorded_days.lock().unwrap().clone()
    }
}

#[async_trait]
impl MaintenanceService for MockMaintenance {
    async fn run_incremental_vacuum(&self) -> Result<IncrementalVacuumOutcome, AppError> {
        *self.calls.lock().unwrap() += 1;
        match &self.result {
            Ok(o) => Ok(*o),
            Err(e) => Err(AppError::Internal(anyhow::anyhow!("{e}"))),
        }
    }

    async fn run_wal_checkpoint(&self) -> Result<WalCheckpointOutcome, AppError> {
        *self.checkpoint_calls.lock().unwrap() += 1;
        match &self.result {
            Ok(_) => Ok(WalCheckpointOutcome {
                busy: self.checkpoint_busy,
                wal_frames: 0,
                checkpointed_frames: 0,
            }),
            Err(e) => Err(AppError::Internal(anyhow::anyhow!("{e}"))),
        }
    }

    async fn run_optimize(&self) -> Result<(), AppError> {
        *self.optimize_calls.lock().unwrap() += 1;
        match &self.result {
            Ok(_) => Ok(()),
            Err(e) => Err(AppError::Internal(anyhow::anyhow!("{e}"))),
        }
    }

    async fn last_maintenance_day(&self) -> Result<Option<chrono::NaiveDate>, AppError> {
        Ok(*self.last_day.lock().unwrap())
    }

    async fn record_maintenance_day(&self, day: chrono::NaiveDate) -> Result<(), AppError> {
        self.recorded_days.lock().unwrap().push(day);
        Ok(())
    }
}

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
}

fn today() -> chrono::NaiveDate {
    chrono::Utc::now().date_naive()
}

// ── run_vacuum unit tests — cover both match arms ────────────────────────────

#[tokio::test]
async fn run_vacuum_logs_when_pages_reclaimed() {
    let repo = MockMaintenance::ok(42);
    run_vacuum(repo.as_ref(), &admin_ctx()).await;
    assert_eq!(repo.call_count(), 1);
}

/// A run that reclaims nothing still goes through the reporting path — it is
/// the case an operator most needs to be able to see, since it is otherwise
/// indistinguishable from the runner not having fired at all.
#[tokio::test]
async fn run_vacuum_reports_when_nothing_reclaimed() {
    let repo = MockMaintenance::ok(0);
    run_vacuum(repo.as_ref(), &admin_ctx()).await;
    assert_eq!(repo.call_count(), 1);
}

/// A stalled run (free pages left behind that could not be relocated) takes
/// the same reporting path; `stop` is what tells the two apart in the log.
#[tokio::test]
async fn run_vacuum_reports_a_stalled_run() {
    let repo = MockMaintenance::with(
        Ok(IncrementalVacuumOutcome {
            reclaimed_pages: 0,
            freelist_before: 100_300,
            freelist_after: 100_300,
            page_count_after: 423_555,
            chunks: 1,
            stop: VacuumStop::Stalled,
        }),
        false,
    );
    run_vacuum(repo.as_ref(), &admin_ctx()).await;
    assert_eq!(repo.call_count(), 1);
}

#[tokio::test]
async fn run_vacuum_warns_and_does_not_panic_on_error() {
    let repo = MockMaintenance::err();
    run_vacuum(repo.as_ref(), &admin_ctx()).await; // must not panic
    assert_eq!(repo.call_count(), 1);
}

// ── run_daily_maintenance — vacuum + checkpoint + optimize ───────────────────

/// The daily sequence must fire all three operations exactly once.
#[tokio::test]
async fn run_daily_maintenance_runs_vacuum_checkpoint_and_optimize() {
    let repo = MockMaintenance::ok(3);
    run_daily_maintenance(repo.as_ref(), &admin_ctx()).await;
    assert_eq!(repo.call_count(), 1, "vacuum should fire once");
    assert_eq!(repo.checkpoint_count(), 1, "checkpoint should fire once");
    assert_eq!(repo.optimize_count(), 1, "optimize should fire once");
}

/// A failure in one step must not stop the others: all three still fire
/// even when every call returns an error.
#[tokio::test]
async fn run_daily_maintenance_continues_past_errors() {
    let repo = MockMaintenance::err();
    run_daily_maintenance(repo.as_ref(), &admin_ctx()).await; // must not panic
    assert_eq!(repo.call_count(), 1);
    assert_eq!(repo.checkpoint_count(), 1);
    assert_eq!(repo.optimize_count(), 1);
}

/// A busy checkpoint (reader held a snapshot) takes the reader-active log
/// branch without panicking and still counts as a call.
#[tokio::test]
async fn run_checkpoint_handles_busy_outcome() {
    let repo = MockMaintenance::ok_busy_checkpoint();
    run_checkpoint(repo.as_ref(), &admin_ctx()).await; // must not panic
    assert_eq!(repo.checkpoint_count(), 1);
}

/// A checkpoint error is logged and swallowed, not propagated.
#[tokio::test]
async fn run_checkpoint_warns_and_does_not_panic_on_error() {
    let repo = MockMaintenance::err();
    run_checkpoint(repo.as_ref(), &admin_ctx()).await; // must not panic
    assert_eq!(repo.checkpoint_count(), 1);
}

// ── Runner integration tests ──────────────────────────────────────────────────

/// Starting and immediately shutting down the runner must complete without
/// panicking. Exercises `DbMaintenanceRunner::start` → `start_with_interval`
/// → `shutdown`, and the cancel branch of `runner_loop`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_shuts_down_cleanly() {
    let repo = MockMaintenance::ok(0);
    repo.ran_on(today());
    let runner = DbMaintenanceRunner::start(repo.clone(), &tracing::Span::none());
    runner.shutdown().await;
}

/// With yesterday's date persisted, the very first tick crosses the day
/// boundary and fires the whole sequence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_fires_vacuum_on_day_rollover() {
    let repo = MockMaintenance::ok(5);
    repo.ran_on(today() - chrono::Days::new(1));

    let runner = DbMaintenanceRunner::start_with_interval(
        repo.clone(),
        Duration::from_millis(20),
        &tracing::Span::none(),
    );

    // Give the ticker enough time to fire at least once.
    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.shutdown().await;

    assert!(
        repo.call_count() >= 1,
        "expected vacuum to fire on day rollover, call_count={}",
        repo.call_count()
    );
    assert!(
        repo.checkpoint_count() >= 1,
        "expected WAL checkpoint to fire on day rollover, checkpoint_count={}",
        repo.checkpoint_count()
    );
    assert!(
        repo.optimize_count() >= 1,
        "expected optimize to fire on day rollover, optimize_count={}",
        repo.optimize_count()
    );
}

/// A database that already ran today must not run again — this is what a
/// restart looks like from the runner's point of view, and it is the half of
/// the schedule that must stay idempotent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_skips_vacuum_when_already_run_today() {
    let repo = MockMaintenance::ok(0);
    repo.ran_on(today());

    let runner = DbMaintenanceRunner::start_with_interval(
        repo.clone(),
        Duration::from_millis(20),
        &tracing::Span::none(),
    );

    tokio::time::sleep(Duration::from_millis(80)).await;
    runner.shutdown().await;

    assert_eq!(
        repo.call_count(),
        0,
        "vacuum must not fire when today's sequence already ran"
    );
}

/// The other half: a database with no record of a run must not wait for the
/// next UTC rollover. Seeding the marker with "today" at startup was what let
/// a box that restarts daily go indefinitely without maintenance.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_fires_when_no_previous_run_is_recorded() {
    let repo = MockMaintenance::ok(7);

    let runner = DbMaintenanceRunner::start_with_interval(
        repo.clone(),
        Duration::from_millis(20),
        &tracing::Span::none(),
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.shutdown().await;

    assert!(
        repo.call_count() >= 1,
        "expected the sequence to run when no previous run is recorded, call_count={}",
        repo.call_count()
    );
}

/// Having run, the runner persists the day — otherwise the next restart would
/// run it all again, and every restart after that.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_records_the_day_after_running() {
    let repo = MockMaintenance::ok(1);

    let runner = DbMaintenanceRunner::start_with_interval(
        repo.clone(),
        Duration::from_millis(20),
        &tracing::Span::none(),
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.shutdown().await;

    let recorded = repo.recorded();
    assert_eq!(
        recorded.first().copied(),
        Some(today()),
        "expected today's date to be persisted, got {recorded:?}"
    );
    assert_eq!(
        recorded.len(),
        1,
        "the sequence must run once per day, got {recorded:?}"
    );
}
