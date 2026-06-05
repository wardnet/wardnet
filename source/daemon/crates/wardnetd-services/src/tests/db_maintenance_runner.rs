use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::db_maintenance_runner::{DbMaintenanceRunner, run_vacuum};
use crate::error::AppError;
use crate::maintenance::MaintenanceService;

// ── Mock maintenance service ──────────────────────────────────────────────────

struct MockMaintenance {
    result: anyhow::Result<u64>,
    calls: Mutex<u32>,
}

impl MockMaintenance {
    fn ok(reclaimed: u64) -> Arc<Self> {
        Arc::new(Self {
            result: Ok(reclaimed),
            calls: Mutex::new(0),
        })
    }

    fn err() -> Arc<Self> {
        Arc::new(Self {
            result: Err(anyhow::anyhow!("synthetic vacuum error")),
            calls: Mutex::new(0),
        })
    }

    fn call_count(&self) -> u32 {
        *self.calls.lock().unwrap()
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
