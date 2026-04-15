//! Tests for the `AdminAuth` and `ClientIp` middleware extractors.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use tower::ServiceExt;
use uuid::Uuid;

use crate::config::Config;
use crate::error::AppError;
use crate::service::auth::LoginResult;
use crate::service::{AuthService, DeviceService};
use crate::state::AppState;
use wardnet_types::api::{DeviceMeResponse, SetMyRuleResponse};
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::routing::RoutingTarget;

use crate::auth_context;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpService, StubDiscoveryService, StubDnsService, StubEventPublisher,
    StubProviderService, StubRoutingService, StubSystemService, StubTunnelService,
};

// ---------------------------------------------------------------------------
// Configurable mock auth service
// ---------------------------------------------------------------------------

/// Mock auth service that returns configurable results for session and API key
/// validation.
struct MockAuthService {
    session_result: Option<Uuid>,
    api_key_result: Option<Uuid>,
}

#[async_trait]
impl AuthService for MockAuthService {
    async fn login(&self, _username: &str, _password: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }

    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(self.session_result)
    }

    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        Ok(self.api_key_result)
    }
    async fn setup_admin(&self, _username: &str, _password: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state(auth: impl AuthService + 'static) -> AppState {
    AppState::new(
        Arc::new(auth),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::dhcp::server::NoopDhcpServer),
        Arc::new(crate::dns::server::NoopDnsServer),
        Arc::new(StubEventPublisher),
        crate::log_broadcast::LogBroadcaster::new(16),
        crate::recent_errors::RecentErrors::new(),
        Config::default(),
        Instant::now(),
    )
}

/// Handler that requires `AdminAuth` and returns the admin UUID.
async fn admin_only(
    crate::api::middleware::AdminAuth { admin_id }: crate::api::middleware::AdminAuth,
) -> impl IntoResponse {
    admin_id.to_string()
}

/// Handler that requires `ClientIp` and returns the IP.
async fn ip_handler(
    crate::api::middleware::ClientIp(ip): crate::api::middleware::ClientIp,
) -> impl IntoResponse {
    ip.to_string()
}

/// Build a router with the admin-only handler.
fn admin_app(state: AppState) -> Router {
    Router::new()
        .route("/test", get(admin_only))
        .with_state(state)
}

fn ip_app(state: AppState) -> Router {
    Router::new()
        .route("/test", get(ip_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// AdminAuth tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_auth_from_session_cookie() {
    let admin_id = Uuid::new_v4();
    let state = make_state(MockAuthService {
        session_result: Some(admin_id),
        api_key_result: None,
    });

    let app = admin_app(state);
    let req = Request::builder()
        .uri("/test")
        .header("Cookie", "wardnet_session=some-token-value")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            1234,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&body), admin_id.to_string());
}

#[tokio::test]
async fn admin_auth_from_bearer_api_key() {
    let admin_id = Uuid::new_v4();
    let state = make_state(MockAuthService {
        session_result: None,
        api_key_result: Some(admin_id),
    });

    let app = admin_app(state);
    let req = Request::builder()
        .uri("/test")
        .header("Authorization", "Bearer my-api-key")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            1234,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&body), admin_id.to_string());
}

#[tokio::test]
async fn admin_auth_rejected_without_credentials() {
    let state = make_state(MockAuthService {
        session_result: None,
        api_key_result: None,
    });

    let app = admin_app(state);
    let req = Request::builder()
        .uri("/test")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            1234,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_auth_session_takes_precedence_over_api_key() {
    let session_id = Uuid::new_v4();
    let api_key_id = Uuid::new_v4();
    let state = make_state(MockAuthService {
        session_result: Some(session_id),
        api_key_result: Some(api_key_id),
    });

    let app = admin_app(state);
    let req = Request::builder()
        .uri("/test")
        .header("Cookie", "wardnet_session=tok")
        .header("Authorization", "Bearer key")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            1234,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    // Session cookie should win.
    assert_eq!(String::from_utf8_lossy(&body), session_id.to_string());
}

#[tokio::test]
async fn admin_auth_ignores_empty_session_cookie() {
    let api_key_id = Uuid::new_v4();
    let state = make_state(MockAuthService {
        session_result: None, // won't be called since cookie is empty
        api_key_result: Some(api_key_id),
    });

    let app = admin_app(state);
    let req = Request::builder()
        .uri("/test")
        .header("Cookie", "wardnet_session=")
        .header("Authorization", "Bearer key")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            1234,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&body), api_key_id.to_string());
}

#[tokio::test]
async fn admin_auth_ignores_empty_bearer_token() {
    let state = make_state(MockAuthService {
        session_result: None,
        api_key_result: None,
    });

    let app = admin_app(state);
    let req = Request::builder()
        .uri("/test")
        .header("Authorization", "Bearer ")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            1234,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// ClientIp tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn client_ip_extracted_from_connect_info() {
    let state = make_state(MockAuthService {
        session_result: None,
        api_key_result: None,
    });

    let app = ip_app(state);
    let req = Request::builder()
        .uri("/test")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42)),
            5555,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&body), "192.168.1.42");
}

#[tokio::test]
async fn client_ip_missing_connect_info_returns_500() {
    let state = make_state(MockAuthService {
        session_result: None,
        api_key_result: None,
    });

    let app = ip_app(state);
    // No ConnectInfo extension.
    let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ---------------------------------------------------------------------------
// Mock device service for resolve_auth_context tests
// ---------------------------------------------------------------------------

/// Mock device service that returns a configurable device for IP lookups.
struct MockDeviceService {
    device: Option<Device>,
}

#[async_trait]
impl DeviceService for MockDeviceService {
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        Ok(DeviceMeResponse {
            device: self.device.clone(),
            current_rule: None,
            admin_locked: false,
            available_tunnels: vec![],
        })
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        _t: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        unimplemented!()
    }
    async fn set_rule(&self, _id: &str, _t: RoutingTarget) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> Result<(), AppError> {
        unimplemented!()
    }
}

fn make_state_with_device(
    auth: impl AuthService + 'static,
    device_svc: impl DeviceService + 'static,
) -> AppState {
    AppState::new(
        Arc::new(auth),
        Arc::new(device_svc),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::dhcp::server::NoopDhcpServer),
        Arc::new(crate::dns::server::NoopDnsServer),
        Arc::new(StubEventPublisher),
        crate::log_broadcast::LogBroadcaster::new(16),
        crate::recent_errors::RecentErrors::new(),
        Config::default(),
        Instant::now(),
    )
}

/// Build a router with `resolve_auth_context` middleware and a handler that
/// echoes the resolved [`AuthContext`] from the task-local.
fn auth_context_app(state: AppState) -> Router {
    Router::new()
        .route("/test", get(echo_auth_context))
        .layer(auth_context::AuthContextLayer)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::api::middleware::resolve_auth_context,
        ))
        .with_state(state)
}

/// Handler that reads the task-local [`AuthContext`] and returns a debug string.
async fn echo_auth_context() -> String {
    let ctx = auth_context::try_current();
    format!("{ctx:?}")
}

// ---------------------------------------------------------------------------
// resolve_auth_context tests
// ---------------------------------------------------------------------------

/// Admin session produces `AuthContext::Admin`.
#[tokio::test]
async fn resolve_auth_context_admin_session() {
    let admin_id = Uuid::new_v4();
    let state = make_state_with_device(
        MockAuthService {
            session_result: Some(admin_id),
            api_key_result: None,
        },
        MockDeviceService { device: None },
    );

    let app = auth_context_app(state);
    let req = Request::builder()
        .uri("/test")
        .header("Cookie", "wardnet_session=tok")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            1234,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("Admin"),
        "expected Admin context, got: {body_str}"
    );
    assert!(
        body_str.contains(&admin_id.to_string()),
        "expected admin_id in context, got: {body_str}"
    );
}

/// Unknown IP with no admin credentials produces `AuthContext::Anonymous`.
#[tokio::test]
async fn resolve_auth_context_unknown_ip_anonymous() {
    let state = make_state_with_device(
        MockAuthService {
            session_result: None,
            api_key_result: None,
        },
        MockDeviceService { device: None },
    );

    let app = auth_context_app(state);
    let req = Request::builder()
        .uri("/test")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 99)),
            5555,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("Anonymous"),
        "expected Anonymous context, got: {body_str}"
    );
}

/// Known device IP produces `AuthContext::Device { mac }`.
#[tokio::test]
async fn resolve_auth_context_known_device_ip() {
    let device = Device {
        id: Uuid::new_v4(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        name: Some("Test Phone".to_owned()),
        hostname: None,
        manufacturer: None,
        device_type: DeviceType::Phone,
        first_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_ip: "192.168.1.42".to_owned(),
        admin_locked: false,
    };
    let state = make_state_with_device(
        MockAuthService {
            session_result: None,
            api_key_result: None,
        },
        MockDeviceService {
            device: Some(device),
        },
    );

    let app = auth_context_app(state);
    let req = Request::builder()
        .uri("/test")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42)),
            5555,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("Device"),
        "expected Device context, got: {body_str}"
    );
    assert!(
        body_str.contains("AA:BB:CC:DD:EE:01"),
        "expected MAC in context, got: {body_str}"
    );
}

/// No `ConnectInfo` and no admin credentials produces `AuthContext::Anonymous`.
#[tokio::test]
async fn resolve_auth_context_no_connect_info_anonymous() {
    let state = make_state_with_device(
        MockAuthService {
            session_result: None,
            api_key_result: None,
        },
        MockDeviceService { device: None },
    );

    let app = auth_context_app(state);
    // No ConnectInfo extension.
    let req = Request::builder().uri("/test").body(Body::empty()).unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("Anonymous"),
        "expected Anonymous context without ConnectInfo, got: {body_str}"
    );
}

// ---------------------------------------------------------------------------
// AdminAuth tests (continued)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn admin_auth_ignores_other_cookies() {
    let state = make_state(MockAuthService {
        session_result: None,
        api_key_result: None,
    });

    let app = admin_app(state);
    let req = Request::builder()
        .uri("/test")
        .header("Cookie", "other_cookie=value; tracking=abc")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            1234,
        )))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // No wardnet_session cookie, no bearer, so should be 401.
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
