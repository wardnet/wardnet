use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
};
use wardnet_common::tunnel::Tunnel;

use wardnetd_services::error::AppError;
use wardnetd_services::tunnel::TunnelService;

use crate::tunnel_monitor::TunnelMonitor;

// -- MockTunnelService -------------------------------------------------------

/// Mock tunnel service that counts calls to `collect_stats` and `run_health_check`.
struct MockTunnelService {
    collect_stats_count: AtomicU32,
    health_check_count: AtomicU32,
    collect_stats_error: AtomicBool,
    health_check_error: AtomicBool,
}

impl MockTunnelService {
    fn new() -> Self {
        Self {
            collect_stats_count: AtomicU32::new(0),
            health_check_count: AtomicU32::new(0),
            collect_stats_error: AtomicBool::new(false),
            health_check_error: AtomicBool::new(false),
        }
    }

    fn with_collect_stats_error() -> Self {
        let svc = Self::new();
        svc.collect_stats_error.store(true, Ordering::SeqCst);
        svc
    }

    fn with_health_check_error() -> Self {
        let svc = Self::new();
        svc.health_check_error.store(true, Ordering::SeqCst);
        svc
    }
}

#[async_trait]
impl TunnelService for MockTunnelService {
    async fn import_tunnel(
        &self,
        _req: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError> {
        unimplemented!()
    }
    async fn get_tunnel(&self, _id: Uuid) -> Result<Tunnel, AppError> {
        unimplemented!()
    }
    async fn bring_up(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn bring_up_internal(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down_internal(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn delete_tunnel(&self, _id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn restore_tunnels(&self) -> Result<(), AppError> {
        unimplemented!()
    }

    async fn collect_stats(&self) -> Result<(), AppError> {
        self.collect_stats_count.fetch_add(1, Ordering::SeqCst);
        if self.collect_stats_error.load(Ordering::SeqCst) {
            return Err(AppError::Internal(anyhow::anyhow!(
                "simulated collect_stats error"
            )));
        }
        Ok(())
    }

    async fn run_health_check(&self) -> Result<(), AppError> {
        self.health_check_count.fetch_add(1, Ordering::SeqCst);
        if self.health_check_error.load(Ordering::SeqCst) {
            return Err(AppError::Internal(anyhow::anyhow!(
                "simulated run_health_check error"
            )));
        }
        Ok(())
    }
}

// -- Tests -------------------------------------------------------------------

#[tokio::test]
async fn stats_loop_calls_collect_stats() {
    let svc = Arc::new(MockTunnelService::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(svc.clone(), 1, 60, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;

    assert!(
        svc.collect_stats_count.load(Ordering::SeqCst) > 0,
        "expected at least one collect_stats call"
    );
}

#[tokio::test]
async fn health_loop_calls_run_health_check() {
    let svc = Arc::new(MockTunnelService::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(svc.clone(), 60, 1, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;

    assert!(
        svc.health_check_count.load(Ordering::SeqCst) > 0,
        "expected at least one run_health_check call"
    );
}

#[tokio::test]
async fn collect_stats_error_does_not_panic() {
    let svc = Arc::new(MockTunnelService::with_collect_stats_error());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(svc.clone(), 1, 60, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;

    // Error is logged but no panic — collect_stats was still called.
    assert!(svc.collect_stats_count.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn health_check_error_does_not_panic() {
    let svc = Arc::new(MockTunnelService::with_health_check_error());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(svc.clone(), 60, 1, &parent);

    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
    monitor.shutdown().await;

    assert!(svc.health_check_count.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn shutdown_stops_both_loops() {
    let svc = Arc::new(MockTunnelService::new());

    let parent = tracing::info_span!("test");
    let monitor = TunnelMonitor::start(svc, 1, 1, &parent);

    // Shutdown immediately — should complete without hanging.
    monitor.shutdown().await;
}
