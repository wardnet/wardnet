//! Tests for [`MaintenanceServiceImpl`] — the auth-gated wrapper over the
//! [`MaintenanceRepository`]. Covers the admin gate and the pass-through of
//! each operation's success and error to the repository.

use std::sync::Arc;

use async_trait::async_trait;
use wardnet_common::auth::AuthContext;
use wardnetd_data::repository::{
    IncrementalVacuumOutcome, MaintenanceRepository, VacuumStop, WalCheckpointOutcome,
};

use crate::auth_context;
use crate::error::AppError;
use crate::maintenance::{MaintenanceService, MaintenanceServiceImpl};

/// Mock repository whose every operation succeeds or fails on demand.
struct MockRepo {
    fail: bool,
    /// Days handed to `record_maintenance_day`, in call order.
    recorded_days: std::sync::Mutex<Vec<chrono::NaiveDate>>,
}

#[async_trait]
impl MaintenanceRepository for MockRepo {
    async fn incremental_vacuum(&self) -> anyhow::Result<IncrementalVacuumOutcome> {
        if self.fail {
            anyhow::bail!("synthetic vacuum error")
        }
        Ok(IncrementalVacuumOutcome {
            reclaimed_pages: 7,
            freelist_before: 7,
            freelist_after: 0,
            page_count_after: 100,
            chunks: 1,
            stop: VacuumStop::Drained,
        })
    }

    async fn wal_checkpoint_truncate(&self) -> anyhow::Result<WalCheckpointOutcome> {
        if self.fail {
            anyhow::bail!("synthetic checkpoint error")
        }
        Ok(WalCheckpointOutcome {
            busy: false,
            wal_frames: 12,
            checkpointed_frames: 12,
        })
    }

    async fn optimize(&self) -> anyhow::Result<()> {
        if self.fail {
            anyhow::bail!("synthetic optimize error")
        }
        Ok(())
    }

    async fn last_maintenance_day(&self) -> anyhow::Result<Option<chrono::NaiveDate>> {
        if self.fail {
            anyhow::bail!("synthetic read error")
        }
        Ok(Some(chrono::NaiveDate::from_ymd_opt(2026, 8, 10).unwrap()))
    }

    async fn record_maintenance_day(&self, day: chrono::NaiveDate) -> anyhow::Result<()> {
        if self.fail {
            anyhow::bail!("synthetic write error")
        }
        self.recorded_days.lock().unwrap().push(day);
        Ok(())
    }

    async fn ping(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn service(fail: bool) -> MaintenanceServiceImpl {
    // The diagnostic-log repositories are unused by these cases; a lazy pool
    // constructs without I/O so they cost nothing here.
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    MaintenanceServiceImpl::new(
        Arc::new(MockRepo {
            fail,
            recorded_days: std::sync::Mutex::new(Vec::new()),
        }),
        Arc::new(wardnetd_data::repository::sqlite::SqliteDeviceEventRepository::new(pool.clone())),
        Arc::new(wardnetd_data::repository::sqlite::SqliteDhcpRepository::new(pool)),
    )
}

fn admin_ctx() -> AuthContext {
    AuthContext::system()
}

// ── run_wal_checkpoint ───────────────────────────────────────────────────────

#[tokio::test]
async fn run_wal_checkpoint_returns_outcome_for_admin() {
    let svc = service(false);
    let outcome = auth_context::with_context(admin_ctx(), svc.run_wal_checkpoint())
        .await
        .expect("admin checkpoint should succeed");
    assert!(!outcome.busy);
    assert_eq!(outcome.checkpointed_frames, 12);
}

#[tokio::test]
async fn run_wal_checkpoint_maps_repo_error_to_internal() {
    let svc = service(true);
    let err = auth_context::with_context(admin_ctx(), svc.run_wal_checkpoint())
        .await
        .expect_err("repo failure should surface");
    assert!(matches!(err, AppError::Internal(_)));
}

#[tokio::test]
async fn run_wal_checkpoint_forbidden_without_admin() {
    let svc = service(false);
    // No auth context set → require_admin() must reject before touching the repo.
    let err = svc
        .run_wal_checkpoint()
        .await
        .expect_err("non-admin must be forbidden");
    assert!(matches!(err, AppError::Forbidden(_)));
}

// ── run_optimize ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn run_optimize_succeeds_for_admin() {
    let svc = service(false);
    auth_context::with_context(admin_ctx(), svc.run_optimize())
        .await
        .expect("admin optimize should succeed");
}

#[tokio::test]
async fn run_optimize_maps_repo_error_to_internal() {
    let svc = service(true);
    let err = auth_context::with_context(admin_ctx(), svc.run_optimize())
        .await
        .expect_err("repo failure should surface");
    assert!(matches!(err, AppError::Internal(_)));
}

#[tokio::test]
async fn run_optimize_forbidden_without_admin() {
    let svc = service(false);
    let err = svc
        .run_optimize()
        .await
        .expect_err("non-admin must be forbidden");
    assert!(matches!(err, AppError::Forbidden(_)));
}

// ── run_incremental_vacuum ────────────────────────────────────────────

#[tokio::test]
async fn run_incremental_vacuum_returns_outcome_for_admin() {
    let svc = service(false);
    let outcome = auth_context::with_context(admin_ctx(), svc.run_incremental_vacuum())
        .await
        .expect("admin vacuum should succeed");
    assert_eq!(outcome.reclaimed_pages, 7);
    assert_eq!(outcome.stop, VacuumStop::Drained);
}

// ── last / record maintenance day ──────────────────────────────────────

#[tokio::test]
async fn last_maintenance_day_returns_stored_day_for_admin() {
    let svc = service(false);
    let day = auth_context::with_context(admin_ctx(), svc.last_maintenance_day())
        .await
        .expect("admin read should succeed");
    assert_eq!(day, chrono::NaiveDate::from_ymd_opt(2026, 8, 10));
}

#[tokio::test]
async fn last_maintenance_day_forbidden_without_admin() {
    let svc = service(false);
    let err = svc
        .last_maintenance_day()
        .await
        .expect_err("non-admin must be forbidden");
    assert!(matches!(err, AppError::Forbidden(_)));
}

#[tokio::test]
async fn record_maintenance_day_succeeds_for_admin() {
    let svc = service(false);
    let day = chrono::NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
    auth_context::with_context(admin_ctx(), svc.record_maintenance_day(day))
        .await
        .expect("admin write should succeed");
}

#[tokio::test]
async fn record_maintenance_day_maps_repo_error_to_internal() {
    let svc = service(true);
    let day = chrono::NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
    let err = auth_context::with_context(admin_ctx(), svc.record_maintenance_day(day))
        .await
        .expect_err("repo failure should surface");
    assert!(matches!(err, AppError::Internal(_)));
}

#[tokio::test]
async fn record_maintenance_day_forbidden_without_admin() {
    let svc = service(false);
    let day = chrono::NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
    let err = svc
        .record_maintenance_day(day)
        .await
        .expect_err("non-admin must be forbidden");
    assert!(matches!(err, AppError::Forbidden(_)));
}

// ── prune_diagnostic_logs ────────────────────────────────────────────────────

/// Both diagnostic logs are pruned by the same daily step, so neither can be
/// left growing because the other's runner happened to be the one wired up.
#[tokio::test]
async fn prune_diagnostic_logs_covers_device_events_and_lease_logs() {
    use wardnet_common::device_event::DeviceEventKind;
    use wardnetd_data::repository::sqlite::{SqliteDeviceEventRepository, SqliteDhcpRepository};
    use wardnetd_data::repository::{DeviceEventRepository, NewDeviceEvent};

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();

    let events = Arc::new(SqliteDeviceEventRepository::new(pool.clone()));
    let ancient = chrono::Utc::now() - chrono::Duration::days(120);
    events
        .record(NewDeviceEvent {
            device_id: "device-1",
            mac: "8c:86:dd:3d:0f:96",
            kind: DeviceEventKind::IpChanged,
            details: None,
            created_at: ancient,
        })
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO dhcp_lease_log (lease_id, mac_address, event_type, created_at) \
         VALUES ('lease-1', '8c:86:dd:3d:0f:96', 'renewed', '2025-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let service = MaintenanceServiceImpl::new(
        Arc::new(MockRepo {
            fail: false,
            recorded_days: std::sync::Mutex::new(Vec::new()),
        }),
        events,
        Arc::new(SqliteDhcpRepository::new(pool.clone())),
    );

    let deleted = auth_context::with_context(admin_ctx(), service.prune_diagnostic_logs())
        .await
        .unwrap();

    assert_eq!(deleted, 2, "one row from each diagnostic log");
}

#[tokio::test]
async fn prune_diagnostic_logs_requires_admin() {
    let service = service(false);
    assert!(service.prune_diagnostic_logs().await.is_err());
}
