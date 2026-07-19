use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::event::WardnetEvent;

use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::health::checks::DotServerHealthCheck;
use crate::health::{CheckOutcome, HealthCheck};
use crate::private_dns::runner::DotRunner;
use crate::private_dns::{DotServer, PrivateDnsGrant, PrivateDnsService, PrivateDnsStatus};
use crate::tls::{TlsService, TlsStatus};

// -- Mock DotServer --------------------------------------------------------

#[derive(Default)]
struct MockDotServer {
    running: AtomicBool,
    starts: AtomicU32,
    stops: AtomicU32,
    disconnected: std::sync::Mutex<Vec<Uuid>>,
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
    fn disconnect_device(&self, device_id: Uuid) {
        self.disconnected.lock().unwrap().push(device_id);
    }
}

// -- Mock PrivateDnsService (only `status` is exercised) -------------------

struct MockPrivateDnsService {
    enabled: AtomicBool,
    /// When set, `is_enabled` returns an error — models a transient config
    /// read failure so the runner's error branch can be exercised.
    fail: AtomicBool,
}

impl MockPrivateDnsService {
    fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            fail: AtomicBool::new(false),
        }
    }

    fn erroring() -> Self {
        let s = Self::new(true);
        s.fail.store(true, Ordering::SeqCst);
        s
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
    async fn is_enabled(&self) -> Result<bool, AppError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(AppError::Internal(anyhow::anyhow!("config read failed")));
        }
        Ok(self.enabled.load(Ordering::SeqCst))
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

/// A `TlsService` whose `status()` always errors — models a transient read
/// failure so the runner's TLS error branch is exercised.
struct ErroringTlsService;

#[async_trait]
impl TlsService for ErroringTlsService {
    async fn status(&self) -> Result<TlsStatus, AppError> {
        Err(AppError::Internal(anyhow::anyhow!("tls read failed")))
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

#[tokio::test]
async fn grant_revoked_event_disconnects_the_device() {
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

    let device_id = Uuid::new_v4();
    events.publish(WardnetEvent::PrivateDnsGrantRevoked {
        device_id,
        timestamp: Utc::now(),
    });

    // The listener is torn down for that device without stopping the server
    // (the feature is still enabled).
    wait_for(|| server.disconnected.lock().unwrap().contains(&device_id)).await;
    assert!(
        server.is_running(),
        "a single revocation must not stop the whole listener"
    );

    runner.shutdown().await;
}

#[tokio::test]
async fn status_read_error_keeps_the_listener_down() {
    // A transient is_enabled() error must resolve desired-state to false —
    // fail safe — not crash the runner or start with unknown state.
    let server = Arc::new(MockDotServer::default());
    let events: Arc<BroadcastEventBus> = Arc::new(BroadcastEventBus::new(16));
    let runner = DotRunner::start(
        Arc::new(MockPrivateDnsService::erroring()),
        Arc::new(MockTlsService::issued()),
        server.clone(),
        events.as_ref(),
        &tracing::Span::none(),
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!server.is_running());
    assert_eq!(server.starts.load(Ordering::SeqCst), 0);
    runner.shutdown().await;
}

#[tokio::test]
async fn tls_read_error_keeps_the_listener_down() {
    // Enabled, but the TLS status read errors: desired-state resolves false
    // so a DDNS/cert read hiccup can't bring a half-known listener up.
    let server = Arc::new(MockDotServer::default());
    let events: Arc<BroadcastEventBus> = Arc::new(BroadcastEventBus::new(16));
    let runner = DotRunner::start(
        Arc::new(MockPrivateDnsService::new(true)),
        Arc::new(ErroringTlsService),
        server.clone(),
        events.as_ref(),
        &tracing::Span::none(),
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!server.is_running());
    runner.shutdown().await;
}

// -- DotServerHealthCheck (desired-vs-actual) ------------------------------

fn is_down(outcome: &CheckOutcome) -> bool {
    matches!(outcome, CheckOutcome::Down { .. })
}

#[tokio::test]
async fn health_down_only_when_enabled_cert_live_but_not_running() {
    let service = Arc::new(MockPrivateDnsService::new(true));
    let tls = Arc::new(MockTlsService::issued());
    let server = Arc::new(MockDotServer::default()); // not running
    let check = DotServerHealthCheck::new(service, tls, server.clone());

    assert_eq!(check.name(), "dot");
    assert!(
        is_down(&check.check().await),
        "enabled + issued cert + not running is a crash → DOWN"
    );

    // Once the listener is up, the same preconditions read UP.
    server.start().await.unwrap();
    assert!(!is_down(&check.check().await));
}

#[tokio::test]
async fn health_up_when_disabled() {
    let service = Arc::new(MockPrivateDnsService::new(false));
    let tls = Arc::new(MockTlsService::issued());
    let server = Arc::new(MockDotServer::default()); // not running, but disabled
    let check = DotServerHealthCheck::new(service, tls, server);
    assert!(
        !is_down(&check.check().await),
        "a disabled feature is UP even with the listener down"
    );
}

#[tokio::test]
async fn health_up_when_cert_not_yet_issued() {
    let service = Arc::new(MockPrivateDnsService::new(true));
    let tls = Arc::new(MockTlsService::not_configured());
    let server = Arc::new(MockDotServer::default()); // not running
    let check = DotServerHealthCheck::new(service, tls, server);
    assert!(
        !is_down(&check.check().await),
        "enabled but no cert yet (normal post-enrollment) is UP, not a crash"
    );
}
