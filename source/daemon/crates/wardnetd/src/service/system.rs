use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use wardnet_types::api::SystemStatusResponse;

use crate::error::AppError;
use crate::repository::SystemConfigRepository;

/// System-wide status and health information.
///
/// Aggregates daemon metadata (version, uptime) with counts from the database
/// to produce a single status snapshot for the admin dashboard.
#[async_trait]
pub trait SystemService: Send + Sync {
    /// Build a status response with version, uptime, and entity counts.
    async fn status(&self) -> Result<SystemStatusResponse, AppError>;
}

/// Default implementation of [`SystemService`] backed by [`SystemConfigRepository`].
pub struct SystemServiceImpl {
    system_config: Arc<dyn SystemConfigRepository>,
    started_at: Instant,
}

impl SystemServiceImpl {
    pub fn new(system_config: Arc<dyn SystemConfigRepository>, started_at: Instant) -> Self {
        Self {
            system_config,
            started_at,
        }
    }
}

#[async_trait]
impl SystemService for SystemServiceImpl {
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        let version = self
            .system_config
            .get("daemon_version")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "unknown".to_owned());

        let device_count = self
            .system_config
            .device_count()
            .await
            .map_err(AppError::Internal)?;

        let tunnel_count = self
            .system_config
            .tunnel_count()
            .await
            .map_err(AppError::Internal)?;

        Ok(SystemStatusResponse {
            version,
            uptime_seconds: self.started_at.elapsed().as_secs(),
            device_count: u64::try_from(device_count).unwrap_or(0),
            tunnel_count: u64::try_from(tunnel_count).unwrap_or(0),
            db_size_bytes: 0,
        })
    }
}
