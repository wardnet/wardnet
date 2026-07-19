use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::event::WardnetEvent;

use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::private_dns::runner::DotRunner;
use crate::private_dns::{DotServer, PrivateDnsGrant, PrivateDnsService, PrivateDnsStatus};
use crate::tls::{TlsService, TlsStatus};

// -- Mock DotServer --------------------------------------------------------

#[derive(Default)]
struct MockDotServer {
    running: AtomicBool,
    starts: AtomicU32,
    stops: AtomicU32,
}

#[async_trait]
impl DotServer for MockDotServer {
    async fn start(&self) -> anyhow::Result<()> {
        self.running.store(true, Ordering::SeqCst);
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn stop(&self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::SeqCst);
        self.stops.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

// -- Mock PrivateDnsService (only `status` is exercised) -------------------

struct MockPrivateDnsService {
    enabled: AtomicBool,
}

impl MockPrivateDnsService {
    fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
        }
    }
}

#[async_trait]
impl PrivateDnsService for MockPrivateDnsService {
    async fn status(&self) -> Result<PrivateDnsStatus, AppError> {
        Ok(PrivateDnsStatus {
            enabled: self.enabled.load(Ordering::SeqCst),
            domain: Some("casa.my.wardnet.services".to_owned()),
        })
    }
    async fn set_enabled(&self, enabled: bool) -> Result<PrivateDnsStatus, AppError> {
        self.enabled.store(enabled, Ordering::SeqCst);
        self.status().await
    }
    async fn grant_device(&self, _device_id: Uuid) -> Result<PrivateDnsGrant, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn revoke_grant(&self, _grant_id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn list_grants(&self) -> Result<Vec<PrivateDnsGrant>, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn resolve_token(&self, _token: &str) -> Result<Option<PrivateDnsGrant>, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        Ok(())
    }
}

// -- Mock TlsService -------------------------------------------------------

struct MockTlsService {
    status: std::sync::Mutex<TlsStatus>,
}

impl MockTlsService {
    fn issued() -> Self {
        Self {
            status: std::sync::Mutex::new(TlsStatus::UpToDate {
                domain: "casa.my.wardnet.services".to_owned(),
                not_after: Utc::now() + chrono::Duration::days(60),
            }),
        }
    }

    fn not_configured() -> Self {
        Self {
            status: std::sync::Mutex::new(TlsStatus::NotConfigured),
        }
    }
}

#[async_trait]
impl TlsService for MockTlsService {
    async fn status(&self) -> Result<TlsStatus, AppError> {
        Ok(self.status.lock().unwrap().clone())
    }
    async fn ensure_certificate(&self) -> Result<TlsStatus, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn mark_provisioning_started(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn provisioning_status(
        &self,
    ) -> Result<wardnet_common::api::TlsStatusResponse, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn teardown(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by runner tests")
    }
}

/// Poll until `predicate` holds or ~2s elapse — the runner reacts to its
/// immediate first tick / events asynchronously.
async fn wait_for(predicate: impl Fn() -> bool) {
    for _ in 0..200 {
        if predicate() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(predicate(), "condition not reached within 2s");
}

#[tokio::test]
async fn starts_the_server_when_enabled_and_cert_is_live() {
    let server = Arc::new(MockDotServer::default());
    let events: Arc<BroadcastEventBus> = Arc::new(BroadcastEventBus::new(16));
    let runner = DotRunner::start(
        Arc::new(MockPrivateDnsService::new(true)),
        Arc::new(MockTlsService::issued()),
        server.clone(),
        events.as_ref(),
        &tracing::Span::none(),
    );

    wait_for(|| server.is_running()).await;
    runner.shutdown().await;
    assert!(!server.is_running(), "shutdown must stop the server");
}

#[tokio::test]
async fn stays_down_while_disabled() {
    let server = Arc::new(MockDotServer::default());
    let events: Arc<BroadcastEventBus> = Arc::new(BroadcastEventBus::new(16));
    let runner = DotRunner::start(
        Arc::new(MockPrivateDnsService::new(false)),
        Arc::new(MockTlsService::issued()),
        server.clone(),
        events.as_ref(),
        &tracing::Span::none(),
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!server.is_running(), "disabled feature must not start :853");
    assert_eq!(server.starts.load(Ordering::SeqCst), 0);
    runner.shutdown().await;
}

#[tokio::test]
async fn stays_down_without_an_issued_cert() {
    let server = Arc::new(MockDotServer::default());
    let events: Arc<BroadcastEventBus> = Arc::new(BroadcastEventBus::new(16));
    let runner = DotRunner::start(
        Arc::new(MockPrivateDnsService::new(true)),
        Arc::new(MockTlsService::not_configured()),
        server.clone(),
        events.as_ref(),
        &tracing::Span::none(),
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !server.is_running(),
        "no cert on :443 means nothing to derive :853 from"
    );
    runner.shutdown().await;
}

#[tokio::test]
async fn reacts_to_private_dns_changed_events() {
    let server = Arc::new(MockDotServer::default());
    let events: Arc<BroadcastEventBus> = Arc::new(BroadcastEventBus::new(16));
    let service = Arc::new(MockPrivateDnsService::new(true));
    let runner = DotRunner::start(
        service.clone(),
        Arc::new(MockTlsService::issued()),
        server.clone(),
        events.as_ref(),
        &tracing::Span::none(),
    );
    wait_for(|| server.is_running()).await;

    // Disable → the event drives the stop without waiting for a tick.
    service.enabled.store(false, Ordering::SeqCst);
    events.publish(WardnetEvent::PrivateDnsChanged {
        enabled: false,
        timestamp: Utc::now(),
    });
    wait_for(|| !server.is_running()).await;

    // Re-enable → the event drives the start again.
    service.enabled.store(true, Ordering::SeqCst);
    events.publish(WardnetEvent::PrivateDnsChanged {
        enabled: true,
        timestamp: Utc::now(),
    });
    wait_for(|| server.is_running()).await;

    runner.shutdown().await;
}

#[tokio::test]
async fn entitlement_loss_event_rechecks_state() {
    let server = Arc::new(MockDotServer::default());
    let events: Arc<BroadcastEventBus> = Arc::new(BroadcastEventBus::new(16));
    let service = Arc::new(MockPrivateDnsService::new(true));
    let runner = DotRunner::start(
        service.clone(),
        Arc::new(MockTlsService::issued()),
        server.clone(),
        events.as_ref(),
        &tracing::Span::none(),
    );
    wait_for(|| server.is_running()).await;

    // The entitlement listener persists the disable through the service;
    // the runner just re-derives on the entitlement edge.
    service.enabled.store(false, Ordering::SeqCst);
    events.publish(WardnetEvent::EntitlementChanged {
        entitled: false,
        timestamp: Utc::now(),
    });
    wait_for(|| !server.is_running()).await;

    runner.shutdown().await;
}
