use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::post;
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
// Stub services (not exercised in these tests)
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
        _t: RoutingTarget,
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
    async fn scan_departures(&self, _t: u64) -> Result<Vec<Uuid>, AppError> {
        unimplemented!()
    }
    async fn resolve_hostname(&self, _id: Uuid, _ip: String) -> Result<(), AppError> {
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
        _n: Option<&str>,
        _dt: Option<DeviceType>,
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
        _r: CreateTunnelRequest,
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
    async fn tear_down(&self, _id: Uuid, _r: &str) -> Result<(), AppError> {
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
// Mock auth service
// ---------------------------------------------------------------------------

/// Mock auth service returning a configurable login result or error.
struct MockAuthService {
    login_result: Result<LoginResult, AppError>,
}

#[async_trait]
impl AuthService for MockAuthService {
    async fn login(&self, _username: &str, _password: &str) -> Result<LoginResult, AppError> {
        match &self.login_result {
            Ok(r) => Ok(LoginResult {
                token: r.token.clone(),
                max_age_seconds: r.max_age_seconds,
            }),
            Err(_) => Err(AppError::Unauthorized("invalid credentials".to_owned())),
        }
    }

    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
    }

    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
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

fn login_app(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/login", post(crate::api::auth::login))
        .with_state(state)
}

fn connect_info_ext() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn login_success_returns_200_and_set_cookie() {
    let state = make_state(MockAuthService {
        login_result: Ok(LoginResult {
            token: "test-session-token".to_owned(),
            max_age_seconds: 86400,
        }),
    });

    let app = login_app(state);
    let body = serde_json::json!({
        "username": "admin",
        "password": "password123"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify Set-Cookie header is present with correct structure.
    let cookie = resp
        .headers()
        .get("set-cookie")
        .expect("expected Set-Cookie header")
        .to_str()
        .unwrap();

    assert!(cookie.contains("wardnet_session=test-session-token"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("Max-Age=86400"));

    // Verify JSON body.
    let resp_body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(json["message"], "logged in");
}

#[tokio::test]
async fn login_failure_returns_401() {
    let state = make_state(MockAuthService {
        login_result: Err(AppError::Unauthorized("invalid credentials".to_owned())),
    });

    let app = login_app(state);
    let body = serde_json::json!({
        "username": "admin",
        "password": "wrong"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_missing_body_returns_422_or_400() {
    let state = make_state(MockAuthService {
        login_result: Ok(LoginResult {
            token: "unused".to_owned(),
            max_age_seconds: 0,
        }),
    });

    let app = login_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // axum returns 422 Unprocessable Entity for deserialization failures.
    assert!(
        resp.status() == StatusCode::UNPROCESSABLE_ENTITY
            || resp.status() == StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn login_invalid_json_returns_error() {
    let state = make_state(MockAuthService {
        login_result: Ok(LoginResult {
            token: "unused".to_owned(),
            max_age_seconds: 0,
        }),
    });

    let app = login_app(state);
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from("not json"))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_client_error());
}
