//! Thin service over cross-cutting database maintenance.
//!
//! Exists so background runners never hold a `Arc<dyn MaintenanceRepository>`
//! directly: like every other background component, [`DbMaintenanceRunner`]
//! calls an auth-gated service under an admin
//! [`crate::auth_context`]. There is exactly one operation today —
//! [`run_incremental_vacuum`](MaintenanceService::run_incremental_vacuum) —
//! mirroring [`wardnetd_data::repository::MaintenanceRepository`].
//!
//! [`DbMaintenanceRunner`]: crate::db_maintenance_runner::DbMaintenanceRunner

use std::sync::Arc;

use async_trait::async_trait;

use wardnetd_data::repository::{MaintenanceRepository, WalCheckpointOutcome};

use crate::auth_context;
use crate::error::AppError;

/// Auth-gated service for cross-cutting database maintenance, so background
/// runners call it under an admin [`auth_context`] instead of holding a
/// [`MaintenanceRepository`] directly.
#[async_trait]
pub trait MaintenanceService: Send + Sync {
    /// Release free database pages back to the filesystem in a single
    /// bounded step. Returns the number of pages reclaimed (best-effort).
    async fn run_incremental_vacuum(&self) -> Result<u64, AppError>;

    /// Truncate the WAL sidecar back to ~0 via
    /// `PRAGMA wal_checkpoint(TRUNCATE)`. Returns the checkpoint outcome so
    /// the caller can log a [`WalCheckpointOutcome::busy`] result.
    async fn run_wal_checkpoint(&self) -> Result<WalCheckpointOutcome, AppError>;

    /// Refresh the query planner's statistics via `PRAGMA optimize`.
    async fn run_optimize(&self) -> Result<(), AppError>;
}

pub struct MaintenanceServiceImpl {
    repo: Arc<dyn MaintenanceRepository>,
}

impl MaintenanceServiceImpl {
    #[must_use]
    pub fn new(repo: Arc<dyn MaintenanceRepository>) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl MaintenanceService for MaintenanceServiceImpl {
    async fn run_incremental_vacuum(&self) -> Result<u64, AppError> {
        auth_context::require_admin()?;
        self.repo
            .incremental_vacuum()
            .await
            .map_err(AppError::Internal)
    }

    async fn run_wal_checkpoint(&self) -> Result<WalCheckpointOutcome, AppError> {
        auth_context::require_admin()?;
        self.repo
            .wal_checkpoint_truncate()
            .await
            .map_err(AppError::Internal)
    }

    async fn run_optimize(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.repo.optimize().await.map_err(AppError::Internal)
    }
}
