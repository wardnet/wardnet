use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_types::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, DeviceMeResponse,
    ListProvidersResponse, ListServersRequest, ListServersResponse, ListTunnelsResponse,
    SetMyRuleResponse, SetupProviderRequest, SetupProviderResponse, SetupStatusResponse,
    SystemStatusResponse, ValidateCredentialsRequest, ValidateCredentialsResponse,
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

struct StubProviderService;
#[async_trait]
impl ProviderService for StubProviderService {
    async fn list_providers(&self) -> Result<ListProvidersResponse, AppError> {
        unimplemented!()
    }
    async fn validate_credentials(
        &self,
        _provider_id: &str,
        _request: ValidateCredentialsRequest,
    ) -> Result<ValidateCredentialsResponse, AppError> {
        unimplemented!()
    }
    async fn list_servers(
        &self,
        _provider_id: &str,
        _request: ListServersRequest,
    ) -> Result<ListServersResponse, AppError> {
        unimplemented!()
    }
    async fn setup_tunnel(
        &self,
        _provider_id: &str,
        _request: SetupProviderRequest,
    ) -> Result<SetupProviderResponse, AppError> {
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

// ---------------------------------------------------------------------------
// Mock auth service for setup tests
// ---------------------------------------------------------------------------

/// Mock auth service that tracks setup state.
struct MockSetupAuthService {
    setup_completed: bool,
    setup_result: Result<(), AppError>,
}

#[async_trait]
impl AuthService for MockSetupAuthService {
    async fn login(&self, _u: &str, _p: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
    }
    async fn setup_admin(&self, _username: &str, _password: &str) -> Result<(), AppError> {
        match &self.setup_result {
            Ok(()) => Ok(()),
            Err(_) => Err(AppError::Conflict("setup already completed".to_owned())),
        }
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        Ok(self.setup_completed)
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

fn setup_app(state: AppState) -> Router {
    Router::new()
        .route("/api/setup/status", get(crate::api::setup::setup_status))
        .route("/api/setup", post(crate::api::setup::setup))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn setup_status_returns_false_initially() {
    let state = make_state(MockSetupAuthService {
        setup_completed: false,
        setup_result: Ok(()),
    });
    let app = setup_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/setup/status")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: SetupStatusResponse = serde_json::from_slice(&body).unwrap();
    assert!(!json.setup_completed);
}

#[tokio::test]
async fn setup_creates_admin_and_returns_201() {
    let state = make_state(MockSetupAuthService {
        setup_completed: false,
        setup_result: Ok(()),
    });
    let app = setup_app(state);

    let body = serde_json::json!({
        "username": "admin",
        "password": "password123"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/setup")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp_body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(
        json["message"],
        "Admin account created. You can now log in."
    );
}

#[tokio::test]
async fn setup_returns_409_when_already_completed() {
    let state = make_state(MockSetupAuthService {
        setup_completed: true,
        setup_result: Err(AppError::Conflict("setup already completed".to_owned())),
    });
    let app = setup_app(state);

    let body = serde_json::json!({
        "username": "admin",
        "password": "password123"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/setup")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn setup_status_returns_true_when_completed() {
    let state = make_state(MockSetupAuthService {
        setup_completed: true,
        setup_result: Ok(()),
    });
    let app = setup_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/setup/status")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: SetupStatusResponse = serde_json::from_slice(&body).unwrap();
    assert!(json.setup_completed);
}
