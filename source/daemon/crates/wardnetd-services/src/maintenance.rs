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

use wardnetd_data::repository::{
    IncrementalVacuumOutcome, MaintenanceRepository, WalCheckpointOutcome,
};

use wardnetd_data::repository::sqlite::format_ts;
use wardnetd_data::repository::{
    DEVICE_EVENT_MAX_PER_DEVICE, DEVICE_EVENT_RETENTION_DAYS, DeviceEventRepository, DhcpRepository,
};

use crate::auth_context;
use crate::error::AppError;

/// Auth-gated service for cross-cutting database maintenance, so background
/// runners call it under an admin [`auth_context`] instead of holding a
/// [`MaintenanceRepository`] directly.
#[async_trait]
pub trait MaintenanceService: Send + Sync {
    /// Release free database pages back to the filesystem in a single
    /// bounded step. Returns the run's outcome so the caller can log what
    /// happened — including the case where nothing was reclaimed.
    async fn run_incremental_vacuum(&self) -> Result<IncrementalVacuumOutcome, AppError>;

    /// Truncate the WAL sidecar back to ~0 via
    /// `PRAGMA wal_checkpoint(TRUNCATE)`. Returns the checkpoint outcome so
    /// the caller can log a [`WalCheckpointOutcome::busy`] result.
    async fn run_wal_checkpoint(&self) -> Result<WalCheckpointOutcome, AppError>;

    /// Refresh the query planner's statistics via `PRAGMA optimize`.
    async fn run_optimize(&self) -> Result<(), AppError>;

    /// The UTC calendar date the daily sequence last completed, or `None` if
    /// it has never run. Read once at startup so a restart resumes the
    /// schedule instead of restarting it.
    async fn last_maintenance_day(&self) -> Result<Option<chrono::NaiveDate>, AppError>;

    /// Record `day` as the date the daily sequence last completed.
    async fn record_maintenance_day(&self, day: chrono::NaiveDate) -> Result<(), AppError>;

    /// Apply retention to the diagnostic logs, returning how many rows went.
    ///
    /// Covers `device_events` (a 30-day age cap plus a per-device row cap, so a
    /// device churning far faster than the age cap can contain still has a
    /// ceiling) and `dhcp_lease_log`, an append-only audit trail that otherwise
    /// grows without bound — a single client stuck at the renewal floor
    /// contributes thousands of rows a day indefinitely.
    async fn prune_diagnostic_logs(&self) -> Result<u64, AppError>;
}

pub struct MaintenanceServiceImpl {
    repo: Arc<dyn MaintenanceRepository>,
    device_events: Arc<dyn DeviceEventRepository>,
    dhcp: Arc<dyn DhcpRepository>,
}

impl MaintenanceServiceImpl {
    #[must_use]
    pub fn new(
        repo: Arc<dyn MaintenanceRepository>,
        device_events: Arc<dyn DeviceEventRepository>,
        dhcp: Arc<dyn DhcpRepository>,
    ) -> Self {
        Self {
            repo,
            device_events,
            dhcp,
        }
    }
}

#[async_trait]
impl MaintenanceService for MaintenanceServiceImpl {
    async fn run_incremental_vacuum(&self) -> Result<IncrementalVacuumOutcome, AppError> {
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

    async fn last_maintenance_day(&self) -> Result<Option<chrono::NaiveDate>, AppError> {
        auth_context::require_admin()?;
        self.repo
            .last_maintenance_day()
            .await
            .map_err(AppError::Internal)
    }

    async fn record_maintenance_day(&self, day: chrono::NaiveDate) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.repo
            .record_maintenance_day(day)
            .await
            .map_err(AppError::Internal)
    }
    async fn prune_diagnostic_logs(&self) -> Result<u64, AppError> {
        auth_context::require_admin()?;

        let cutoff = chrono::Utc::now() - chrono::Duration::days(DEVICE_EVENT_RETENTION_DAYS);
        let events = self
            .device_events
            .prune(cutoff, DEVICE_EVENT_MAX_PER_DEVICE)
            .await
            .map_err(AppError::Internal)?;
        let leases = self
            .dhcp
            .prune_lease_logs(&format_ts(cutoff))
            .await
            .map_err(AppError::Internal)?;

        Ok(events + leases)
    }
}
