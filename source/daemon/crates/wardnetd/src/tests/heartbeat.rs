use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use wardnet_common::api::{
    DhcpSelfProbeResponse, DiscoverGatewayMacRequest, DiscoverGatewayMacResponse,
    NetworkStatusResponse, SystemStatusResponse,
};
use wardnetd_services::auth_context;
use wardnetd_services::error::AppError;
use wardnetd_services::system::SystemService;

use crate::heartbeat::HeartbeatRunner;

/// Mock system service that counts `record_heartbeat` calls and
/// optionally returns an error so the runner's failure-swallowing
/// branch is exercised.
struct MockSystemService {
    heartbeat_count: AtomicU32,
    fail: AtomicBool,
}

impl MockSystemService {
    fn new() -> Self {
        Self {
            heartbeat_count: AtomicU32::new(0),
            fail: AtomicBool::new(false),
        }
    }

    fn failing() -> Self {
        let svc = Self::new();
        svc.fail.store(true, Ordering::SeqCst);
        svc
    }
}

#[async_trait]
impl SystemService for MockSystemService {
    fn version(&self) -> &'static str {
        "0.0.0-test"
    }
    fn uptime(&self) -> Duration {
        Duration::ZERO
    }
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        unimplemented!()
    }
    async fn network_status(&self) -> Result<NetworkStatusResponse, AppError> {
        unimplemented!()
    }
    async fn discover_gateway_mac(
        &self,
        _request: DiscoverGatewayMacRequest,
    ) -> Result<DiscoverGatewayMacResponse, AppError> {
        unimplemented!()
    }
    async fn dhcp_self_probe(&self) -> Result<DhcpSelfProbeResponse, AppError> {
        unimplemented!()
    }
    async fn request_restart(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn request_reboot(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn request_shutdown(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn record_heartbeat(&self) -> Result<(), AppError> {
        // Heartbeat goes through `auth_context::with_context`, so the
        // service-layer admin guard must see an Admin context here.
        auth_context::require_admin()?;
        self.heartbeat_count.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(AppError::Internal(anyhow::anyhow!("simulated db error")));
        }
        Ok(())
    }
    async fn record_graceful_shutdown(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn acknowledge_last_shutdown(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

#[tokio::test]
async fn fires_initial_heartbeat_on_start() {
    let svc = Arc::new(MockSystemService::new());
    let parent = tracing::info_span!("test");
    let runner = HeartbeatRunner::start_with_interval(svc.clone(), Duration::from_mins(1), &parent);

    // The runner fires once immediately on startup. Yield + a small
    // sleep so the spawned task has a chance to run.
    tokio::time::sleep(Duration::from_millis(50)).await;
    runner.shutdown().await;

    assert!(
        svc.heartbeat_count.load(Ordering::SeqCst) >= 1,
        "expected at least one heartbeat fire on startup",
    );
}

#[tokio::test]
async fn ticks_periodically() {
    let svc = Arc::new(MockSystemService::new());
    let parent = tracing::info_span!("test");
    let runner =
        HeartbeatRunner::start_with_interval(svc.clone(), Duration::from_millis(50), &parent);

    // Wait long enough for two or three ticks past the initial fire.
    tokio::time::sleep(Duration::from_millis(220)).await;
    runner.shutdown().await;

    assert!(
        svc.heartbeat_count.load(Ordering::SeqCst) >= 2,
        "expected periodic ticks beyond the initial fire, got {}",
        svc.heartbeat_count.load(Ordering::SeqCst)
    );
}

#[tokio::test]
async fn swallows_repository_errors() {
    let svc = Arc::new(MockSystemService::failing());
    let parent = tracing::info_span!("test");
    let runner =
        HeartbeatRunner::start_with_interval(svc.clone(), Duration::from_millis(50), &parent);

    tokio::time::sleep(Duration::from_millis(180)).await;
    runner.shutdown().await;

    // Initial fire + at least one tick — both surfaced errors and
    // neither brought the runner down.
    assert!(svc.heartbeat_count.load(Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn shutdown_stops_loop_promptly() {
    let svc = Arc::new(MockSystemService::new());
    let parent = tracing::info_span!("test");
    let runner = HeartbeatRunner::start_with_interval(svc, Duration::from_hours(1), &parent);

    // Even with a 1-hour tick, shutdown returns within the cancellation
    // poll window because the loop selects on `cancel.cancelled()`.
    let started = std::time::Instant::now();
    runner.shutdown().await;
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "shutdown should not wait on the next tick: took {:?}",
        started.elapsed()
    );
}
