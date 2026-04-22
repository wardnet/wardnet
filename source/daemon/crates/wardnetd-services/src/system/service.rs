use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sysinfo::System;
use tokio_util::sync::CancellationToken;
use wardnet_common::api::SystemStatusResponse;

use crate::auth_context;
use crate::error::AppError;
use wardnetd_data::repository::{SystemConfigRepository, TunnelRepository};

/// How long the restart handler waits before cancelling the shutdown
/// token. Lets the HTTP response flush so the client sees
/// `204 No Content` before the graceful-shutdown path starts draining
/// connections. Anything longer isn't needed; anything shorter
/// occasionally cuts the response short on a busy connection.
const RESTART_GRACE: Duration = Duration::from_millis(500);

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

    /// Ask the daemon to exit cleanly so the supervisor (systemd on a
    /// Pi install, the operator on dev) brings it back up.
    ///
    /// The call returns immediately; a background task performs the
    /// actual `exit(0)` after a short grace period so the HTTP
    /// response completes before the socket closes. Admin-only.
    async fn request_restart(&self) -> Result<(), AppError>;
}

/// Default implementation of [`SystemService`] backed by [`SystemConfigRepository`].
///
/// Holds a persistent `sysinfo::System` instance behind a `tokio::sync::Mutex`
/// so that successive calls to `status()` produce accurate CPU readings
/// (sysinfo needs two measurement points to compute CPU usage).
///
/// Also carries the daemon's shutdown [`CancellationToken`].
/// `request_restart` cancels it after a short grace period, which in
/// turn triggers the `with_graceful_shutdown` arm in `main.rs`
/// (see `shutdown_signal`) — axum finishes draining in-flight
/// requests and the background runners shut down cleanly via their
/// own `shutdown().await` sequence. Plumbing the same token through
/// here keeps `std::process::exit` out of the codebase.
pub struct SystemServiceImpl {
    system_config: Arc<dyn SystemConfigRepository>,
    tunnel_repo: Arc<dyn TunnelRepository>,
    started_at: Instant,
    sysinfo: tokio::sync::Mutex<System>,
    shutdown_token: CancellationToken,
}

impl SystemServiceImpl {
    pub fn new(
        system_config: Arc<dyn SystemConfigRepository>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        started_at: Instant,
        shutdown_token: CancellationToken,
    ) -> Self {
        Self {
            system_config,
            tunnel_repo,
            started_at,
            sysinfo: tokio::sync::Mutex::new(System::new_all()),
            shutdown_token,
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

    async fn request_restart(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::warn!(
            grace_ms = RESTART_GRACE.as_millis(),
            "daemon restart requested via API — triggering graceful shutdown in {grace_ms}ms",
            grace_ms = RESTART_GRACE.as_millis(),
        );
        let token = self.shutdown_token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(RESTART_GRACE).await;
            // Cancelling the token wakes `shutdown_signal` in main.rs
            // which returns from `axum::serve.with_graceful_shutdown`.
            // The main task then runs the existing runner.shutdown()
            // sequence before the process exits with code 0.
            // `Restart=always` in wardnetd.service brings the daemon
            // back up on a Pi install; on the dev mock the process
            // exits cleanly and the operator re-runs `make run-dev`.
            token.cancel();
        });
        Ok(())
    }
}
