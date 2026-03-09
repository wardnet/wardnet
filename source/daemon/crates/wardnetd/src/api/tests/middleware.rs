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
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_types::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, DeviceMeResponse,
    ListTunnelsResponse, SetMyRuleResponse, SystemStatusResponse,
};
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::event::WardnetEvent;
use wardnet_types::routing::RoutingTarget;
use wardnet_types::tunnel::Tunnel;

use crate::config::Config;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::packet_capture::ObservedDevice;
use crate::service::auth::LoginResult;
use wardnet_types::api::{
    ListProvidersResponse, ListServersRequest, ListServersResponse, SetupProviderRequest,
    SetupProviderResponse, ValidateCredentialsRequest, ValidateCredentialsResponse,
};

use crate::service::{
    AuthService, DeviceDiscoveryService, DeviceService, ObservationResult, ProviderService,
    SystemService, TunnelService,
};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Stub services (satisfy AppState::new but are not exercised in these tests)
// ---------------------------------------------------------------------------

struct StubDeviceService;
#[async_trait]
impl DeviceService for StubDeviceService {
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        unimplemented!()
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        _target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        unimplemented!()
    }
}

struct StubDiscoveryService;
#[async_trait]
impl DeviceDiscoveryService for StubDiscoveryService {
    async fn restore_devices(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn process_observation(
        &self,
        _obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
        unimplemented!()
    }
    async fn flush_last_seen(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
    async fn scan_departures(&self, _timeout_secs: u64) -> Result<Vec<Uuid>, AppError> {
        unimplemented!()
    }
    async fn resolve_hostname(&self, _device_id: Uuid, _ip: String) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn get_all_devices(&self) -> Result<Vec<Device>, AppError> {
        unimplemented!()
    }
    async fn get_device_by_id(&self, _id: Uuid) -> Result<Device, AppError> {
        unimplemented!()
    }
    async fn update_device(
        &self,
        _id: Uuid,
        _name: Option<&str>,
        _device_type: Option<DeviceType>,
    ) -> Result<Device, AppError> {
        unimplemented!()
    }
}

struct StubSystemService;
#[async_trait]
impl SystemService for StubSystemService {
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        unimplemented!()
    }
}

struct StubTunnelService;
#[async_trait]
impl TunnelService for StubTunnelService {
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
    async fn delete_tunnel(&self, _id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn restore_tunnels(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

struct StubEventPublisher;
impl EventPublisher for StubEventPublisher {
    fn publish(&self, _event: WardnetEvent) {}
    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        let (tx, rx) = broadcast::channel(1);
        drop(tx);
        rx
    }
}

struct StubProviderService;
#[async_trait]
impl ProviderService for StubProviderService {
    async fn list_providers(&self) -> Result<ListProvidersResponse, AppError> {
        Ok(ListProvidersResponse { providers: vec![] })
    }
    async fn validate_credentials(
        &self,
        _id: &str,
        _req: ValidateCredentialsRequest,
    ) -> Result<ValidateCredentialsResponse, AppError> {
        unimplemented!()
    }
    async fn list_servers(
        &self,
        _id: &str,
        _req: ListServersRequest,
    ) -> Result<ListServersResponse, AppError> {
        unimplemented!()
    }
    async fn setup_tunnel(
        &self,
        _id: &str,
        _req: SetupProviderRequest,
    ) -> Result<SetupProviderResponse, AppError> {
        unimplemented!()
    }
}

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
        Arc::new(StubDiscoveryService),
        Arc::new(StubProviderService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubEventPublisher),
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
