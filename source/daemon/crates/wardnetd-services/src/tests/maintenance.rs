//! Tests for [`MaintenanceServiceImpl`] — the auth-gated wrapper over the
//! [`MaintenanceRepository`]. Covers the admin gate and the pass-through of
//! each operation's success and error to the repository.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnetd_data::repository::{MaintenanceRepository, WalCheckpointOutcome};

use crate::auth_context;
use crate::error::AppError;
use crate::maintenance::{MaintenanceService, MaintenanceServiceImpl};

/// Mock repository whose every operation succeeds or fails on demand.
struct MockRepo {
    fail: bool,
}

#[async_trait]
impl MaintenanceRepository for MockRepo {
    async fn incremental_vacuum(&self) -> anyhow::Result<u64> {
        if self.fail {
            anyhow::bail!("synthetic vacuum error")
        }
        Ok(7)
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

    async fn ping(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

fn service(fail: bool) -> MaintenanceServiceImpl {
    MaintenanceServiceImpl::new(Arc::new(MockRepo { fail }))
}

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
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

// ── run_incremental_vacuum (pre-existing method, exercised for completeness) ──

#[tokio::test]
async fn run_incremental_vacuum_returns_reclaimed_for_admin() {
    let svc = service(false);
    let reclaimed = auth_context::with_context(admin_ctx(), svc.run_incremental_vacuum())
        .await
        .expect("admin vacuum should succeed");
    assert_eq!(reclaimed, 7);
}
