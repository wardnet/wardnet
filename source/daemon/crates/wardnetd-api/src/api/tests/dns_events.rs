//! Tests for the DNS events SSE + ack API endpoints.
//! GET  /api/devices/me/dns-events/stream
//! POST /api/devices/me/dns-events/ack

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use tower::ServiceExt;
use wardnet_common::api::{
    DeviceMeResponse, DnsCaptureSettingsResponse, DnsEventItem, SetMyRuleResponse,
};
use wardnet_common::auth::AuthContext;
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::routing::RoutingTarget;

use crate::state::AppState;
use crate::tests::stubs::{
    StubDhcpServer, StubDhcpService, StubDnsFilterService, StubDnsServer, StubDnsService,
    StubEventPublisher, StubLogService, StubNetworkZoneService, StubProviderService,
    StubRoutingService, StubSystemService, StubTunnelService,
};
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_common::auth::{AuthenticatedUser, UserRole};
use wardnet_common::event::WardnetEvent;
use wardnet_test_support::principal;
use wardnetd_services::DeviceService;
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::auth::{CurrentUser, LoginAttempt};
use wardnetd_services::error::AppError;
use wardnetd_services::event::EventPublisher;

// ---------------------------------------------------------------------------
// LagPublisher — 1-slot broadcast that triggers RecvError::Lagged
// ---------------------------------------------------------------------------

struct LagPublisher {
    tx: Arc<broadcast::Sender<WardnetEvent>>,
    subscribed: Arc<tokio::sync::Notify>,
}

impl EventPublisher for LagPublisher {
    fn publish(&self, event: WardnetEvent) {
        let _ = self.tx.send(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        self.subscribed.notify_one();
        self.tx.subscribe()
    }
}

// ---------------------------------------------------------------------------
// MockAuthService — always validates sessions as admin
// ---------------------------------------------------------------------------

struct MockAuthService;

#[async_trait]
impl wardnetd_services::AuthService for MockAuthService {
    async fn current_user(&self) -> Result<CurrentUser, AppError> {
        Ok(CurrentUser {
            user_id: Uuid::nil(),
            display_name: "admin".to_owned(),
            email: None,
            role: UserRole::Admin,
        })
    }
    async fn login(&self, _attempt: LoginAttempt<'_>) -> Result<LoginResult, AppError> {
        Ok(LoginResult {
            token: "t".to_owned(),
            max_age_seconds: 3600,
        })
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(Some(principal::admin(
            Uuid::parse_str("00000000-0000-0000-0000-000000000099").unwrap(),
        )))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(None)
    }
    async fn setup_admin(&self, _u: &str, _p: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        unimplemented!()
    }
    async fn wizard_state(
        &self,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        unimplemented!()
    }
    async fn advance_wizard(
        &self,
        _to_step: wardnet_common::api::WizardStep,
        _mode: Option<wardnet_common::api::WizardMode>,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        unimplemented!()
    }
    async fn logout_session(&self, _token: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// MockDnsEventsDeviceService
// ---------------------------------------------------------------------------

fn sample_device() -> Device {
    Device {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        name: None,
        hostname: Some("device-1".to_owned()),
        manufacturer: None,
        manufacturer_source: None,
        is_randomized: false,
        device_type: DeviceType::Unknown,
        first_seen: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
        last_ip: "192.168.1.100".to_owned(),
        admin_locked: false,
        zone_id: "00000000-0000-0000-0000-000000000201".parse().unwrap(),
        owner_user_id: None,
        dns_capture_enabled: true,
        dns_capture_cap_count: 500,
        dns_capture_cap_days: 14,
        connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
        managed: false,
    }
}

struct MockDnsEventsDeviceService {
    /// Device to return from `get_device_for_ip`. `None` → 404.
    device: Option<Device>,
    /// Rows returned by `fetch_pending_dns_events`.
    pending: Vec<DnsEventItem>,
    /// When `true`, `fetch_pending_dns_events` returns an error.
    fetch_error: bool,
    /// Records the `AuthContext` observed inside `fetch_pending_dns_events`, so
    /// a test can assert the SSE handler propagated the request's identity into
    /// the detached flush task.
    observed_ctx: Arc<std::sync::Mutex<Option<AuthContext>>>,
}

impl MockDnsEventsDeviceService {
    fn with_device(pending: Vec<DnsEventItem>) -> Self {
        Self {
            device: Some(sample_device()),
            pending,
            fetch_error: false,
            observed_ctx: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    fn not_found() -> Self {
        Self {
            device: None,
            pending: vec![],
            fetch_error: false,
            observed_ctx: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    fn with_fetch_error() -> Self {
        Self {
            device: Some(sample_device()),
            pending: vec![],
            fetch_error: true,
            observed_ctx: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Handle to the slot recording the context seen by the flush task.
    fn ctx_recorder(&self) -> Arc<std::sync::Mutex<Option<AuthContext>>> {
        self.observed_ctx.clone()
    }
}

#[async_trait]
impl DeviceService for MockDnsEventsDeviceService {
    async fn clear_rule(&self, _device_id: &str) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }

    async fn mark_managed(
        &self,
        _device_id: &str,
    ) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }

    async fn clear_managed(
        &self,
        _device_id: &str,
    ) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }

    async fn set_device_owner(
        &self,
        _device_id: &str,
        _owner_user_id: Option<uuid::Uuid>,
    ) -> Result<(), AppError> {
        unimplemented!()
    }

    async fn get_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<wardnet_common::device::Device>, AppError> {
        Ok(self.device.clone())
    }
    async fn clear_remote_connection_mode(&self, _device_id: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        match &self.device {
            Some(d) => Ok(DeviceMeResponse {
                device: Some(d.clone()),
                current_rule: None,
                admin_locked: false,
                available_tunnels: vec![],
                zone: None,
                routing_profiles: vec![],
            }),
            None => Err(AppError::NotFound("device not found".to_owned())),
        }
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        _target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        unimplemented!()
    }
    async fn set_rule(&self, _id: &str, _t: RoutingTarget) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn current_rules(
        &self,
    ) -> Result<std::collections::HashMap<Uuid, RoutingTarget>, AppError> {
        unimplemented!()
    }
    async fn get_rule_for_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<RoutingTarget>, AppError> {
        unimplemented!()
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn get_dns_capture_settings(
        &self,
        _id: &str,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn update_dns_capture_settings(
        &self,
        _id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn set_my_capture_enabled(
        &self,
        _ip: &str,
        _enabled: bool,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn fetch_pending_dns_events(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> Result<Vec<DnsEventItem>, AppError> {
        *self.observed_ctx.lock().unwrap() = wardnetd_services::auth_context::try_current();
        if self.fetch_error {
            return Err(AppError::Internal(anyhow::anyhow!("db error")));
        }
        Ok(self.pending.clone())
    }
    async fn ack_dns_events(&self, _device_id: &str, _up_to_id: i64) -> Result<(), AppError> {
        Ok(())
    }
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        Ok(vec![])
    }
    async fn get_device_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// Live event publisher — wraps a real broadcast channel so tests can emit events
// ---------------------------------------------------------------------------

// LiveEventPublisher holds a Weak reference to the sender so that when the
// test drops its Arc<Sender>, the broadcast channel closes and the SSE
// handler's live loop exits via RecvError::Closed — preventing a hang.
struct LiveEventPublisher {
    tx: std::sync::Weak<broadcast::Sender<WardnetEvent>>,
    subscribed: Arc<tokio::sync::Notify>,
}

impl LiveEventPublisher {
    fn new() -> (Self, Arc<broadcast::Sender<WardnetEvent>>) {
        let (tx, _) = broadcast::channel(64);
        let tx = Arc::new(tx);
        let publisher = Self {
            tx: Arc::downgrade(&tx),
            subscribed: Arc::new(tokio::sync::Notify::new()),
        };
        (publisher, tx)
    }

    fn subscribed_notify(&self) -> Arc<tokio::sync::Notify> {
        Arc::clone(&self.subscribed)
    }
}

impl EventPublisher for LiveEventPublisher {
    fn publish(&self, event: WardnetEvent) {
        if let Some(tx) = self.tx.upgrade() {
            let _ = tx.send(event);
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        self.subscribed.notify_one();
        self.tx
            .upgrade()
            .expect("sender dropped before subscribe")
            .subscribe()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_state(device_svc: impl DeviceService + 'static) -> AppState {
    build_state_with_publisher(device_svc, StubEventPublisher)
}

fn build_state_with_publisher(
    device_svc: impl DeviceService + 'static,
    publisher: impl EventPublisher + 'static,
) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(device_svc),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(crate::tests::stubs::StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(crate::tests::stubs::StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(publisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubRuleRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    )
}

fn dns_events_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/devices/me/dns-events/stream",
            get(crate::api::dns_events::stream_dns_events),
        )
        .route(
            "/api/devices/me/dns-events/ack",
            post(crate::api::dns_events::ack_dns_events),
        )
        .with_state(state)
}

fn client_connect_info() -> axum::extract::ConnectInfo<std::net::SocketAddr> {
    axum::extract::ConnectInfo(std::net::SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(192, 168, 1, 100)),
        12345,
    ))
}

/// Send a GET and return the status + first 4 KB of body.
async fn get_raw(app: Router, uri: &str) -> (StatusCode, String) {
    let resp = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .extension(client_connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body_bytes = axum::body::to_bytes(resp.into_body(), 4096)
        .await
        .unwrap_or_default();
    let body = String::from_utf8_lossy(&body_bytes).to_string();
    (status, body)
}

/// Send a POST with JSON body and return status + response body.
async fn post_json(app: Router, uri: &str, json_body: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Content-Type", "application/json")
                .extension(client_connect_info())
                .body(Body::from(json_body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stream_returns_sse_content_type() {
    let state = build_state(MockDnsEventsDeviceService::with_device(vec![]));
    let app = dns_events_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/devices/me/dns-events/stream")
                .extension(client_connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("text/event-stream"),
        "expected text/event-stream, got: {ct}"
    );
}

#[tokio::test]
async fn stream_emits_flush_event_from_mock() {
    let pending = vec![DnsEventItem {
        id: 7,
        domain: "ads.tracker.io".to_owned(),
        status: "blocked".to_owned(),
        captured_at: "2026-06-12T00:00:00Z".to_owned(),
    }];
    let state = build_state(MockDnsEventsDeviceService::with_device(pending));
    let app = dns_events_router(state);

    let (status, body) = get_raw(app, "/api/devices/me/dns-events/stream").await;

    assert_eq!(status, StatusCode::OK);
    // The flush phase emits SSE data lines; at least one should contain our domain.
    assert!(
        body.contains("ads.tracker.io"),
        "expected flushed event in SSE body, got: {body}"
    );
    assert!(
        body.contains("id: 7"),
        "expected SSE event id: 7, got: {body}"
    );
}

#[tokio::test]
async fn stream_propagates_request_auth_context_to_flush_task() {
    let pending = vec![DnsEventItem {
        id: 9,
        domain: "captured.example".to_owned(),
        status: "allowed".to_owned(),
        captured_at: "2026-06-12T00:00:00Z".to_owned(),
    }];
    let svc = MockDnsEventsDeviceService::with_device(pending);
    let recorder = svc.ctx_recorder();
    let app = dns_events_router(build_state(svc));

    let admin_ctx =
        principal::admin_context(Uuid::parse_str("00000000-0000-0000-0000-0000000000aa").unwrap());

    // Drive the request inside an admin context. The flush task is spawned
    // detached, so it does NOT inherit this task-local — the only way the mock
    // observes the admin context is if the handler captured it and re-wrapped
    // the task in `with_context`. Dropping that wrap would leave the flush task
    // context-less and fail this assertion.
    let resp = wardnetd_services::auth_context::with_context(
        admin_ctx,
        app.oneshot(
            Request::builder()
                .uri("/api/devices/me/dns-events/stream")
                .extension(client_connect_info())
                .body(Body::empty())
                .unwrap(),
        ),
    )
    .await
    .unwrap();

    // Reading the body drives the flush phase to completion, so the mock's
    // fetch call (and its context capture) has run by the time we assert.
    let _ = axum::body::to_bytes(resp.into_body(), 4096)
        .await
        .unwrap_or_default();

    let observed = recorder.lock().unwrap().clone();
    assert!(
        matches!(observed, Some(AuthContext::User(_))),
        "flush task must run under the request's admin context, got: {observed:?}"
    );
}

#[tokio::test]
async fn stream_returns_404_for_unknown_ip() {
    let state = build_state(MockDnsEventsDeviceService::not_found());
    let app = dns_events_router(state);

    let (status, _) = get_raw(app, "/api/devices/me/dns-events/stream").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ack_returns_204() {
    let state = build_state(MockDnsEventsDeviceService::with_device(vec![]));
    let app = dns_events_router(state);

    let (status, _) = post_json(app, "/api/devices/me/dns-events/ack", r#"{"up_to_id":5}"#).await;

    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn ack_returns_404_for_unknown_ip() {
    let state = build_state(MockDnsEventsDeviceService::not_found());
    let app = dns_events_router(state);

    let (status, json) =
        post_json(app, "/api/devices/me/dns-events/ack", r#"{"up_to_id":5}"#).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn ack_returns_422_for_malformed_body() {
    let state = build_state(MockDnsEventsDeviceService::with_device(vec![]));
    let app = dns_events_router(state);

    let (status, _) = post_json(
        app,
        "/api/devices/me/dns-events/ack",
        r#"{"not_a_field":"oops"}"#,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn stream_delivers_live_event_from_bus() {
    // Build a publisher that signals when the spawned task subscribes.
    let (publisher, event_tx) = LiveEventPublisher::new();
    let subscribed = publisher.subscribed_notify();
    let device_uuid = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let state =
        build_state_with_publisher(MockDnsEventsDeviceService::with_device(vec![]), publisher);
    let app = dns_events_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/devices/me/dns-events/stream")
                .extension(client_connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Wait until the spawned task has subscribed to the bus, then emit an
    // event that matches this device so the live-phase arm is exercised.
    subscribed.notified().await;
    let _ = event_tx.send(WardnetEvent::DnsEventInserted {
        device_id: device_uuid,
        row_id: 42,
        domain: "live.example.com".to_owned(),
        status: "allowed".to_owned(),
        captured_at: "2026-06-12T00:00:00Z".to_owned(),
        timestamp: chrono::Utc::now(),
    });
    // Also emit an event for a different device to exercise the Ok(_) arm.
    let _ = event_tx.send(WardnetEvent::DnsEventInserted {
        device_id: uuid::Uuid::nil(),
        row_id: 43,
        domain: "other.device.com".to_owned(),
        status: "blocked".to_owned(),
        captured_at: "2026-06-12T00:00:01Z".to_owned(),
        timestamp: chrono::Utc::now(),
    });
    // Drop sender to close the bus so the live loop exits.
    drop(event_tx);

    let body_bytes = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let body = String::from_utf8_lossy(&body_bytes);
    assert!(
        body.contains("live.example.com"),
        "expected live event in SSE body, got: {body}"
    );
    assert!(
        !body.contains("other.device.com"),
        "event for another device must not appear in stream"
    );
}

#[tokio::test]
async fn stream_closes_on_client_disconnect_without_matching_event() {
    // Idle device: no pending rows to flush and no DnsEventInserted for this
    // device is ever published, so the live loop can only end when the client
    // hangs up. Drive `pump_dns_events` directly rather than through the HTTP
    // handler (which spawns the task detached) so the test owns the JoinHandle
    // and can await termination deterministically instead of polling.
    let (publisher, event_tx) = LiveEventPublisher::new();
    let subscribed = publisher.subscribed_notify();
    let state =
        build_state_with_publisher(MockDnsEventsDeviceService::with_device(vec![]), publisher);

    // Keep the broadcast sender alive for the whole test so the only way the loop
    // can exit is `tx.closed()` — dropping it would let the task leave via
    // `RecvError::Closed` instead, defeating the point of the test.
    let _event_tx = event_tx;

    let device_uuid = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel::<
        Result<axum::response::sse::Event, std::convert::Infallible>,
    >(64);

    let handle = tokio::spawn(crate::api::dns_events::pump_dns_events(
        state,
        device_uuid,
        device_uuid.to_string(),
        0,
        tx,
    ));

    // Wait until the task subscribes; with no pending rows it then falls straight
    // into the live loop with no matching event to forward.
    subscribed.notified().await;

    // Simulate closing the tab: dropping the mpsc receiver closes the channel,
    // which resolves `tx.closed()` inside the live loop's `select!`.
    drop(rx);

    // The task must observe the disconnect and return promptly, even though no
    // matching event was ever published. A `recv()`-only loop would park here
    // forever; the timeout turns that regression into a failure instead of a hang.
    let joined = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    assert!(
        matches!(joined, Ok(Ok(()))),
        "SSE pump task did not terminate after client disconnect: {joined:?}"
    );
}

#[tokio::test]
async fn stream_stops_when_flush_send_fails() {
    // Two pending rows but a 1-slot mpsc channel: the first flush send fills the
    // buffer, the second blocks (the receiver is never drained), and dropping the
    // receiver fails that send so the flush phase returns before the live loop.
    let pending = vec![
        DnsEventItem {
            id: 1,
            domain: "a.example".to_owned(),
            status: "allowed".to_owned(),
            captured_at: "2026-06-12T00:00:00Z".to_owned(),
        },
        DnsEventItem {
            id: 2,
            domain: "b.example".to_owned(),
            status: "blocked".to_owned(),
            captured_at: "2026-06-12T00:00:01Z".to_owned(),
        },
    ];
    let state = build_state(MockDnsEventsDeviceService::with_device(pending));

    let device_uuid = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel::<
        Result<axum::response::sse::Event, std::convert::Infallible>,
    >(1);

    let handle = tokio::spawn(crate::api::dns_events::pump_dns_events(
        state,
        device_uuid,
        device_uuid.to_string(),
        0,
        tx,
    ));

    // Let the flush send the first row and park on the blocked second send.
    tokio::task::yield_now().await;

    // Client hangs up mid-flush: the in-flight send fails and the pump returns.
    drop(rx);

    let joined = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    assert!(
        matches!(joined, Ok(Ok(()))),
        "pump did not stop after flush send failure: {joined:?}"
    );
}

#[tokio::test]
async fn stream_stops_when_live_event_send_fails() {
    // A matching live event is forwarded, but the client hangs up mid-delivery so
    // the `tx.send(...)` in the live loop fails and the pump returns. A 1-slot mpsc
    // channel makes this deterministic: the first event fills the buffer, the
    // second event's send blocks (the receiver is never drained), and dropping the
    // receiver then fails that in-flight send.
    let (publisher, event_tx) = LiveEventPublisher::new();
    let subscribed = publisher.subscribed_notify();
    let state =
        build_state_with_publisher(MockDnsEventsDeviceService::with_device(vec![]), publisher);

    let device_uuid = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel::<
        Result<axum::response::sse::Event, std::convert::Infallible>,
    >(1);

    let handle = tokio::spawn(crate::api::dns_events::pump_dns_events(
        state,
        device_uuid,
        device_uuid.to_string(),
        0,
        tx,
    ));

    subscribed.notified().await;

    let make_event = |row_id: i64| WardnetEvent::DnsEventInserted {
        device_id: device_uuid,
        row_id,
        domain: "live.example.com".to_owned(),
        status: "allowed".to_owned(),
        captured_at: "2026-06-12T00:00:00Z".to_owned(),
        timestamp: chrono::Utc::now(),
    };
    // First event fills the single buffer slot; the second event's send then
    // blocks because the receiver is never drained.
    let _ = event_tx.send(make_event(1));
    let _ = event_tx.send(make_event(2));

    // Let the pump forward the first event and park on the blocked second send.
    tokio::task::yield_now().await;

    // Client hangs up: dropping the receiver fails the in-flight send, so the loop
    // returns via the `tx.send(...).is_err()` path rather than `tx.closed()`.
    drop(rx);

    let joined = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    assert!(
        matches!(joined, Ok(Ok(()))),
        "pump did not stop after live-event send failure: {joined:?}"
    );
}

#[tokio::test]
async fn stream_closes_on_bus_lag() {
    // Use a channel with capacity 1 so we can force a lag condition by
    // publishing 2 messages before the subscriber drains any.
    let (tx, _rx) = broadcast::channel::<WardnetEvent>(1);
    let tx = Arc::new(tx);
    let subscribed = Arc::new(tokio::sync::Notify::new());

    let publisher = LagPublisher {
        tx: Arc::clone(&tx),
        subscribed: Arc::clone(&subscribed),
    };
    let state =
        build_state_with_publisher(MockDnsEventsDeviceService::with_device(vec![]), publisher);
    let app = dns_events_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/devices/me/dns-events/stream")
                .extension(client_connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Wait for subscription, then overflow the 1-slot buffer with 2 sends
    // so the next recv returns Lagged.
    subscribed.notified().await;
    let fake_event = WardnetEvent::DnsEventInserted {
        device_id: uuid::Uuid::nil(),
        row_id: 1,
        domain: "a.com".to_owned(),
        status: "ok".to_owned(),
        captured_at: "2026-06-12T00:00:00Z".to_owned(),
        timestamp: chrono::Utc::now(),
    };
    let _ = tx.send(fake_event.clone());
    let _ = tx.send(fake_event);

    // Keep `tx` ALIVE through the drain. A live sender makes `RecvError::Closed`
    // impossible, so the stream can only close if the `RecvError::Lagged => break`
    // arm actually fires. If that arm regresses to `continue`, the live loop
    // blocks forever on an open-but-empty bus and the timeout below (not a false
    // pass) is what fails the test.
    let drained = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        axum::body::to_bytes(resp.into_body(), 4096),
    )
    .await
    .expect("stream did not close on lag — the RecvError::Lagged arm no longer breaks the loop");
    drained.unwrap();

    // Sender is still open here; dropping it now is only cleanup.
    drop(tx);
}

#[tokio::test]
async fn stream_closes_on_fetch_error() {
    let state = build_state(MockDnsEventsDeviceService::with_fetch_error());
    let app = dns_events_router(state);

    let (status, body) = get_raw(app, "/api/devices/me/dns-events/stream").await;
    // Handler returns 200 + SSE headers; the spawned task closes the channel
    // immediately on fetch error so the body is empty.
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.is_empty() || !body.contains("data:"),
        "body should have no SSE data on error"
    );
}
