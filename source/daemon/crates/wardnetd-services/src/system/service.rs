use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use sysinfo::System;
use wardnet_common::api::SystemStatusResponse;

use crate::auth_context;
use crate::error::AppError;
use wardnetd_data::repository::{SystemConfigRepository, TunnelRepository};

/// System-wide status and health information.
///
/// Aggregates daemon metadata (version, uptime) with counts from the database
/// and host resource usage (CPU, memory) to produce a single status snapshot
/// for the admin dashboard.
#[async_trait]
pub trait SystemService: Send + Sync {
    /// Return the daemon version string. No authentication required.
    fn version(&self) -> &'static str;

    /// Return the daemon uptime. No authentication required.
    fn uptime(&self) -> std::time::Duration;

    /// Build a status response with version, uptime, entity counts, and host resource usage.
    async fn status(&self) -> Result<SystemStatusResponse, AppError>;
}

/// Default implementation of [`SystemService`] backed by [`SystemConfigRepository`].
///
/// Holds a persistent `sysinfo::System` instance behind a `tokio::sync::Mutex`
/// so that successive calls to `status()` produce accurate CPU readings
/// (sysinfo needs two measurement points to compute CPU usage).
pub struct SystemServiceImpl {
    system_config: Arc<dyn SystemConfigRepository>,
    tunnel_repo: Arc<dyn TunnelRepository>,
    started_at: Instant,
    sysinfo: tokio::sync::Mutex<System>,
}

impl SystemServiceImpl {
    pub fn new(
        system_config: Arc<dyn SystemConfigRepository>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        started_at: Instant,
    ) -> Self {
        Self {
            system_config,
            tunnel_repo,
            started_at,
            sysinfo: tokio::sync::Mutex::new(System::new_all()),
        }
    }
}

#[async_trait]
impl SystemService for SystemServiceImpl {
    fn version(&self) -> &'static str {
        env!("WARDNET_VERSION")
    }

    fn uptime(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        auth_context::require_admin()?;

        let version = self.version().to_owned();

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

        let db_size_bytes = self
            .system_config
            .db_size_bytes()
            .await
            .map_err(AppError::Internal)?;

        // Refresh CPU and memory readings from the persistent sysinfo instance.
        let (cpu_usage_percent, memory_used_bytes, memory_total_bytes) = {
            let mut sys = self.sysinfo.lock().await;
            sys.refresh_cpu_all();
            sys.refresh_memory();
            (
                sys.global_cpu_usage(),
                sys.used_memory(),
                sys.total_memory(),
            )
        };

        Ok(SystemStatusResponse {
            version,
            uptime_seconds: self.started_at.elapsed().as_secs(),
            device_count: u64::try_from(device_count).unwrap_or(0),
            tunnel_count: u64::try_from(tunnel_count).unwrap_or(0),
            tunnel_active_count: u64::try_from(
                self.tunnel_repo
                    .count_active()
                    .await
                    .map_err(AppError::Internal)?,
            )
            .unwrap_or(0),
            db_size_bytes,
            cpu_usage_percent,
            memory_used_bytes,
            memory_total_bytes,
        })
    }
}
