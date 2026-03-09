use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_types::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, DeviceMeResponse,
    ListProvidersResponse, ListServersRequest, ListServersResponse, ListTunnelsResponse,
    SetMyRuleResponse, SetupProviderRequest, SetupProviderResponse, SystemStatusResponse,
    ValidateCredentialsRequest, ValidateCredentialsResponse,
};
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::event::WardnetEvent;
use wardnet_types::routing::RoutingTarget;
use wardnet_types::tunnel::{Tunnel, TunnelStatus};
use wardnet_types::vpn_provider::{ProviderAuthMethod, ProviderInfo, ServerInfo};

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
// Stub services (non-provider)
// ---------------------------------------------------------------------------

struct MockAuthService;
#[async_trait]
impl AuthService for MockAuthService {
    async fn login(&self, _u: &str, _p: &str) -> Result<LoginResult, AppError> {
        Ok(LoginResult {
            token: "t".to_owned(),
            max_age_seconds: 3600,
        })
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(
            Uuid::parse_str("00000000-0000-0000-0000-000000000099").unwrap(),
        ))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
    }
    async fn setup_admin(&self, _u: &str, _p: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        Ok(true)
    }
}

struct StubDeviceService;
#[async_trait]
impl DeviceService for StubDeviceService {
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        Ok(DeviceMeResponse {
            device: None,
            current_rule: None,
            admin_locked: false,
        })
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        Ok(SetMyRuleResponse {
            message: "ok".to_owned(),
            target,
        })
    }
}

struct StubDiscoveryService;
#[async_trait]
impl DeviceDiscoveryService for StubDiscoveryService {
    async fn restore_devices(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn process_observation(
        &self,
        _obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
        unimplemented!()
    }
    async fn flush_last_seen(&self) -> Result<u64, AppError> {
        Ok(0)
    }
    async fn scan_departures(&self, _timeout_secs: u64) -> Result<Vec<Uuid>, AppError> {
        Ok(vec![])
    }
    async fn resolve_hostname(&self, _id: Uuid, _ip: String) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_all_devices(&self) -> Result<Vec<Device>, AppError> {
        Ok(vec![])
    }
    async fn get_device_by_id(&self, id: Uuid) -> Result<Device, AppError> {
        Err(AppError::NotFound(format!("device {id} not found")))
    }
    async fn update_device(
        &self,
        id: Uuid,
        _name: Option<&str>,
        _dt: Option<DeviceType>,
    ) -> Result<Device, AppError> {
        Err(AppError::NotFound(format!("device {id} not found")))
    }
}

struct StubSystemService;
#[async_trait]
impl SystemService for StubSystemService {
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        Ok(SystemStatusResponse {
            version: "test".to_owned(),
            uptime_seconds: 0,
            device_count: 0,
            tunnel_count: 0,
            db_size_bytes: 0,
            cpu_usage_percent: 0.0,
            memory_used_bytes: 0,
            memory_total_bytes: 0,
        })
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
        Ok(ListTunnelsResponse { tunnels: vec![] })
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
        Ok(())
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
// Mock provider service
// ---------------------------------------------------------------------------

/// Mock `ProviderService` with configurable responses.
struct MockProviderService {
    providers: Vec<ProviderInfo>,
}

impl MockProviderService {
    fn with_provider() -> Self {
        Self {
            providers: vec![ProviderInfo {
                id: "test-vpn".to_owned(),
                name: "Test VPN".to_owned(),
                auth_methods: vec![ProviderAuthMethod::Credentials],
                icon_url: None,
                website_url: None,
            }],
        }
    }
}

#[async_trait]
impl ProviderService for MockProviderService {
    async fn list_providers(&self) -> Result<ListProvidersResponse, AppError> {
        Ok(ListProvidersResponse {
            providers: self.providers.clone(),
        })
    }

    async fn validate_credentials(
        &self,
        _provider_id: &str,
        _request: ValidateCredentialsRequest,
    ) -> Result<ValidateCredentialsResponse, AppError> {
        Ok(ValidateCredentialsResponse {
            valid: true,
            message: "credentials are valid".to_owned(),
        })
    }

    async fn list_servers(
        &self,
        _provider_id: &str,
        _request: ListServersRequest,
    ) -> Result<ListServersResponse, AppError> {
        Ok(ListServersResponse {
            servers: vec![ServerInfo {
                id: "se-142".to_owned(),
                name: "Sweden #142".to_owned(),
                country_code: "SE".to_owned(),
                city: Some("Stockholm".to_owned()),
                hostname: "se142.testvpn.com".to_owned(),
                load: 23,
            }],
        })
    }

    async fn setup_tunnel(
        &self,
        _provider_id: &str,
        _request: SetupProviderRequest,
    ) -> Result<SetupProviderResponse, AppError> {
        let tunnel = Tunnel {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000010").unwrap(),
            label: "Test VPN - Sweden #142".to_owned(),
            country_code: "SE".to_owned(),
            provider: Some("test-vpn".to_owned()),
            interface_name: "wg_ward0".to_owned(),
            endpoint: "1.2.3.4:51820".to_owned(),
            status: TunnelStatus::Down,
            last_handshake: None,
            bytes_tx: 0,
            bytes_rx: 0,
            created_at: "2026-03-07T00:00:00Z".parse().unwrap(),
        };
        Ok(SetupProviderResponse {
            tunnel,
            server: ServerInfo {
                id: "se-142".to_owned(),
                name: "Sweden #142".to_owned(),
                country_code: "SE".to_owned(),
                city: Some("Stockholm".to_owned()),
                hostname: "se142.testvpn.com".to_owned(),
                load: 23,
            },
            message: "tunnel created via Test VPN (test-vpn)".to_owned(),
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345))
}

fn build_state(provider_svc: impl ProviderService + 'static) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(StubDeviceService),
        Arc::new(StubDiscoveryService),
        Arc::new(provider_svc),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubEventPublisher),
        Config::default(),
        Instant::now(),
    )
}

fn provider_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/providers",
            get(crate::api::providers::list_providers),
        )
        .route(
            "/api/providers/{id}/validate",
            post(crate::api::providers::validate_credentials),
        )
        .route(
            "/api/providers/{id}/servers",
            post(crate::api::providers::list_servers),
        )
        .route(
            "/api/providers/{id}/setup",
            post(crate::api::providers::setup_tunnel),
        )
        .with_state(state)
}

/// Send an authenticated GET request.
async fn get_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Send an authenticated POST request with JSON body.
async fn post_json(app: Router, uri: &str, json_body: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from(json_body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ---------------------------------------------------------------------------
// GET /api/providers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_providers_returns_json() {
    let state = build_state(MockProviderService::with_provider());
    let app = provider_router(state);

    let (status, json) = get_json(app, "/api/providers").await;
    assert_eq!(status, StatusCode::OK);
    let providers = json["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0]["id"], "test-vpn");
    assert_eq!(providers[0]["name"], "Test VPN");
}

#[tokio::test]
async fn list_providers_unauthorized_without_session() {
    let state = build_state(MockProviderService::with_provider());
    let app = provider_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/providers")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// POST /api/providers/:id/validate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn validate_credentials_returns_json() {
    let state = build_state(MockProviderService::with_provider());
    let app = provider_router(state);

    let body = serde_json::json!({
        "credentials": {
            "type": "credentials",
            "username": "user",
            "password": "pass"
        }
    });

    let (status, json) = post_json(
        app,
        "/api/providers/test-vpn/validate",
        &body.to_string(),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["valid"], true);
    assert_eq!(json["message"], "credentials are valid");
}

#[tokio::test]
async fn validate_credentials_unauthorized_without_session() {
    let state = build_state(MockProviderService::with_provider());
    let app = provider_router(state);

    let body = serde_json::json!({
        "credentials": {
            "type": "credentials",
            "username": "user",
            "password": "pass"
        }
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/providers/test-vpn/validate")
                .header("Content-Type", "application/json")
                .extension(connect_info())
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// POST /api/providers/:id/servers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_servers_returns_json() {
    let state = build_state(MockProviderService::with_provider());
    let app = provider_router(state);

    let body = serde_json::json!({
        "credentials": {
            "type": "credentials",
            "username": "user",
            "password": "pass"
        },
        "filter": {
            "country": "SE"
        }
    });

    let (status, json) = post_json(
        app,
        "/api/providers/test-vpn/servers",
        &body.to_string(),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let servers = json["servers"].as_array().unwrap();
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0]["id"], "se-142");
    assert_eq!(servers[0]["country_code"], "SE");
    assert_eq!(servers[0]["load"], 23);
}

// ---------------------------------------------------------------------------
// POST /api/providers/:id/setup
// ---------------------------------------------------------------------------

#[tokio::test]
async fn setup_tunnel_returns_json() {
    let state = build_state(MockProviderService::with_provider());
    let app = provider_router(state);

    let body = serde_json::json!({
        "credentials": {
            "type": "credentials",
            "username": "user",
            "password": "pass"
        },
        "country": "SE"
    });

    let (status, json) = post_json(
        app,
        "/api/providers/test-vpn/setup",
        &body.to_string(),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["tunnel"]["label"], "Test VPN - Sweden #142");
    assert_eq!(json["tunnel"]["country_code"], "SE");
    assert_eq!(json["server"]["id"], "se-142");
    assert!(json["message"].as_str().unwrap().contains("Test VPN"));
}

#[tokio::test]
async fn setup_tunnel_unauthorized_without_session() {
    let state = build_state(MockProviderService::with_provider());
    let app = provider_router(state);

    let body = serde_json::json!({
        "credentials": {
            "type": "credentials",
            "username": "user",
            "password": "pass"
        },
        "country": "SE"
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/providers/test-vpn/setup")
                .header("Content-Type", "application/json")
                .extension(connect_info())
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
