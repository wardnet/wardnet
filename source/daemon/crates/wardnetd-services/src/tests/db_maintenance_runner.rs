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
use wardnetd_data::repository::WalCheckpointOutcome;

// ── Mock maintenance service ──────────────────────────────────────────────────

struct MockMaintenance {
    result: anyhow::Result<u64>,
    /// When the service is `ok`, whether the checkpoint reports `busy`
    /// (a reader blocked the truncation) rather than a clean truncate.
    checkpoint_busy: bool,
    /// Calls to `run_incremental_vacuum`.
    calls: Mutex<u32>,
    /// Calls to `run_wal_checkpoint`.
    checkpoint_calls: Mutex<u32>,
    /// Calls to `run_optimize`.
    optimize_calls: Mutex<u32>,
}

impl MockMaintenance {
    fn ok(reclaimed: u64) -> Arc<Self> {
        Arc::new(Self {
            result: Ok(reclaimed),
            checkpoint_busy: false,
            calls: Mutex::new(0),
            checkpoint_calls: Mutex::new(0),
            optimize_calls: Mutex::new(0),
        })
    }

    /// Like [`ok`](Self::ok) but the checkpoint reports `busy` so the
    /// runner's reader-active log branch is exercised.
    fn ok_busy_checkpoint() -> Arc<Self> {
        Arc::new(Self {
            result: Ok(0),
            checkpoint_busy: true,
            calls: Mutex::new(0),
            checkpoint_calls: Mutex::new(0),
            optimize_calls: Mutex::new(0),
        })
    }

    fn err() -> Arc<Self> {
        Arc::new(Self {
            result: Err(anyhow::anyhow!("synthetic vacuum error")),
            checkpoint_busy: false,
            calls: Mutex::new(0),
            checkpoint_calls: Mutex::new(0),
            optimize_calls: Mutex::new(0),
        })
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
}

#[async_trait]
impl MaintenanceService for MockMaintenance {
    async fn run_incremental_vacuum(&self) -> Result<u64, AppError> {
        *self.calls.lock().unwrap() += 1;
        match &self.result {
            Ok(n) => Ok(*n),
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
}

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
}

// ── run_vacuum unit tests — cover all three match arms ───────────────────────

#[tokio::test]
async fn run_vacuum_logs_when_pages_reclaimed() {
    let repo = MockMaintenance::ok(42);
    run_vacuum(repo.as_ref(), &admin_ctx()).await;
    assert_eq!(repo.call_count(), 1);
}

#[tokio::test]
async fn run_vacuum_silent_when_nothing_reclaimed() {
    let repo = MockMaintenance::ok(0);
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
/// → `start_with_interval_and_day` → `shutdown`, and the cancel branch of
/// `runner_loop`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_shuts_down_cleanly() {
    let repo = MockMaintenance::ok(0);
    let runner = DbMaintenanceRunner::start(repo.clone(), &tracing::Span::none());
    runner.shutdown().await;
}

/// When the runner is started with yesterday's date as the initial day,
/// the very first tick (after a short interval) crosses the day boundary
/// and fires a vacuum.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_fires_vacuum_on_day_rollover() {
    let repo = MockMaintenance::ok(5);
    let yesterday = chrono::Utc::now().date_naive() - chrono::Days::new(1);

    let runner = DbMaintenanceRunner::start_with_interval_and_day(
        repo.clone(),
        Duration::from_millis(20),
        yesterday,
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

/// When the initial day is today, ticks must not fire a vacuum.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_skips_vacuum_when_same_day() {
    let repo = MockMaintenance::ok(0);

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
        "vacuum must not fire when the day has not rolled over"
    );
}
