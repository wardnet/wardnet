use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_types::api::{
    CreateDhcpReservationRequest, CreateDhcpReservationResponse, DeleteDhcpReservationResponse,
    DhcpConfigResponse, DhcpStatusResponse, ListDhcpLeasesResponse, ListDhcpReservationsResponse,
    RevokeDhcpLeaseResponse, ToggleDhcpRequest, UpdateDhcpConfigRequest,
};
use wardnet_types::dhcp::{DhcpConfig, DhcpLease};

use crate::dhcp::runner::DhcpRunner;
use crate::dhcp::server::DhcpServer;
use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::service::DhcpService;

// ---------------------------------------------------------------------------
// Mock DhcpServer for runner tests
// ---------------------------------------------------------------------------

/// Mock server that tracks start/stop calls.
struct MockDhcpServer {
    started: AtomicBool,
    start_count: AtomicU64,
    stop_count: AtomicU64,
}

impl MockDhcpServer {
    fn new() -> Self {
        Self {
            started: AtomicBool::new(false),
            start_count: AtomicU64::new(0),
            stop_count: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl DhcpServer for MockDhcpServer {
    async fn start(&self) -> Result<(), AppError> {
        self.started.store(true, Ordering::SeqCst);
        self.start_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> Result<(), AppError> {
        self.started.store(false, Ordering::SeqCst);
        self.stop_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------
// Mock DhcpService for runner tests
// ---------------------------------------------------------------------------

/// Minimal mock service that returns a config and tracks cleanup calls.
struct MockRunnerDhcpService {
    enabled: bool,
    cleanup_count: AtomicU64,
}

impl MockRunnerDhcpService {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            cleanup_count: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl DhcpService for MockRunnerDhcpService {
    async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _r: UpdateDhcpConfigRequest,
    ) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(&self, _r: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError> {
        unimplemented!()
    }
    async fn revoke_lease(&self, _id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError> {
        unimplemented!()
    }
    async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError> {
        unimplemented!()
    }
    async fn create_reservation(
        &self,
        _r: CreateDhcpReservationRequest,
    ) -> Result<CreateDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn delete_reservation(
        &self,
        _id: Uuid,
    ) -> Result<DeleteDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<DhcpStatusResponse, AppError> {
        unimplemented!()
    }
    async fn assign_lease(
        &self,
        _mac: &str,
        _hostname: Option<&str>,
    ) -> Result<DhcpLease, AppError> {
        unimplemented!()
    }
    async fn renew_lease(&self, _mac: &str) -> Result<DhcpLease, AppError> {
        unimplemented!()
    }
    async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        self.cleanup_count.fetch_add(1, Ordering::SeqCst);
        Ok(0)
    }
    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
        Ok(DhcpConfig {
            enabled: self.enabled,
            pool_start: Ipv4Addr::new(192, 168, 1, 100),
            pool_end: Ipv4Addr::new(192, 168, 1, 200),
            subnet_mask: Ipv4Addr::new(255, 255, 255, 0),
            upstream_dns: vec![Ipv4Addr::new(1, 1, 1, 1)],
            lease_duration_secs: 86400,
            router_ip: Some(Ipv4Addr::new(192, 168, 1, 1)),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_starts_server_when_dhcp_enabled() {
    let service: Arc<dyn DhcpService> = Arc::new(MockRunnerDhcpService::new(true));
    let server = Arc::new(MockDhcpServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let server_dyn: Arc<dyn DhcpServer> = Arc::clone(&server) as Arc<dyn DhcpServer>;
    let runner = DhcpRunner::start(service, server_dyn, events.as_ref(), &parent);

    // Wait for the spawned task to call server.start().
    for _ in 0..40 {
        tokio::task::yield_now().await;
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        if server.start_count.load(Ordering::SeqCst) > 0 {
            break;
        }
    }

    assert_eq!(
        server.start_count.load(Ordering::SeqCst),
        1,
        "DHCP server should have been started"
    );

    runner.shutdown().await;

    assert_eq!(
        server.stop_count.load(Ordering::SeqCst),
        1,
        "DHCP server should have been stopped on shutdown"
    );
}

#[tokio::test]
async fn runner_does_not_start_server_when_dhcp_disabled() {
    let service: Arc<dyn DhcpService> = Arc::new(MockRunnerDhcpService::new(false));
    let server = Arc::new(MockDhcpServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let runner = DhcpRunner::start(
        service,
        Arc::clone(&server) as Arc<dyn DhcpServer>,
        events.as_ref(),
        &parent,
    );

    // Give the runner a moment to decide not to start.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    assert_eq!(server.start_count.load(Ordering::SeqCst), 0);

    runner.shutdown().await;
}

#[tokio::test]
async fn mock_server_start_increments_counter() {
    let server = Arc::new(MockDhcpServer::new());
    let server_dyn: Arc<dyn DhcpServer> = Arc::clone(&server) as Arc<dyn DhcpServer>;
    server_dyn.start().await.unwrap();
    assert_eq!(server.start_count.load(Ordering::SeqCst), 1);
    assert!(server.is_running());
}

#[tokio::test]
async fn runner_shutdown_is_idempotent() {
    let service: Arc<dyn DhcpService> = Arc::new(MockRunnerDhcpService::new(false));
    let server = Arc::new(MockDhcpServer::new());
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::Span::none();

    let runner = DhcpRunner::start(service, server, events.as_ref(), &parent);

    // Shutdown should complete without panic.
    runner.shutdown().await;
}
