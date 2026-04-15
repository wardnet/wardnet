//! Tests for the tunnel API endpoints (GET/POST /api/tunnels, DELETE /api/tunnels/:id).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{delete, get};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_types::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
};
use wardnet_types::tunnel::{Tunnel, TunnelStatus};

use crate::config::Config;
use crate::error::AppError;
use crate::service::auth::LoginResult;
use crate::service::{AuthService, TunnelService};
use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpService, StubDiscoveryService, StubDnsService, StubEventPublisher,
    StubProviderService, StubRoutingService, StubSystemService,
};

// ---------------------------------------------------------------------------
// Mock / stub services
// ---------------------------------------------------------------------------

/// Mock auth service that always validates sessions.
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
    async fn setup_admin(&self, _username: &str, _password: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        unimplemented!()
    }
}

/// Mock `TunnelService` with configurable tunnel list.
struct MockTunnelService {
    tunnels: Vec<Tunnel>,
}

impl MockTunnelService {
    fn with_tunnels(tunnels: Vec<Tunnel>) -> Self {
        Self { tunnels }
    }

    fn empty() -> Self {
        Self { tunnels: vec![] }
    }
}

#[async_trait]
impl TunnelService for MockTunnelService {
    async fn import_tunnel(
        &self,
        req: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError> {
        let tunnel = Tunnel {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000010").unwrap(),
            label: req.label,
            country_code: req.country_code,
            provider: req.provider,
            interface_name: "wg_ward0".to_owned(),
            endpoint: "1.2.3.4:51820".to_owned(),
            status: TunnelStatus::Down,
            last_handshake: None,
            bytes_tx: 0,
            bytes_rx: 0,
            created_at: chrono::Utc::now(),
        };
        Ok(CreateTunnelResponse {
            tunnel,
            message: "tunnel imported successfully".to_owned(),
        })
    }

    async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError> {
        Ok(ListTunnelsResponse {
            tunnels: self.tunnels.clone(),
        })
    }

    async fn get_tunnel(&self, id: Uuid) -> Result<Tunnel, AppError> {
        self.tunnels
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("tunnel {id} not found")))
    }

    async fn bring_up(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }

    async fn tear_down(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
        Ok(())
    }

    async fn bring_up_internal(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }

    async fn tear_down_internal(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
        Ok(())
    }

    async fn delete_tunnel(&self, id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
        if self.tunnels.iter().any(|t| t.id == id) {
            Ok(DeleteTunnelResponse {
                message: "tunnel deleted".to_owned(),
            })
        } else {
            Err(AppError::NotFound(format!("tunnel {id} not found")))
        }
    }

    async fn restore_tunnels(&self) -> Result<(), AppError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sample_tunnel() -> Tunnel {
    Tunnel {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000010").unwrap(),
        label: "US West".to_owned(),
        country_code: "US".to_owned(),
        provider: Some("Mullvad".to_owned()),
        interface_name: "wg_ward0".to_owned(),
        endpoint: "1.2.3.4:51820".to_owned(),
        status: TunnelStatus::Down,
        last_handshake: None,
        bytes_tx: 0,
        bytes_rx: 0,
        created_at: "2026-03-07T00:00:00Z".parse().unwrap(),
    }
}

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345))
}

fn build_state(tunnel_svc: impl TunnelService + 'static) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(tunnel_svc),
        Arc::new(crate::dhcp::server::NoopDhcpServer),
        Arc::new(crate::dns::server::NoopDnsServer),
        Arc::new(StubEventPublisher),
        crate::log_broadcast::LogBroadcaster::new(16),
        crate::recent_errors::RecentErrors::new(),
        Config::default(),
        Instant::now(),
    )
}

fn tunnel_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/tunnels",
            get(crate::api::tunnels::list_tunnels).post(crate::api::tunnels::create_tunnel),
        )
        .route(
            "/api/tunnels/{id}",
            delete(crate::api::tunnels::delete_tunnel),
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

/// Send an authenticated DELETE request.
async fn delete_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
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

// ---------------------------------------------------------------------------
// GET /api/tunnels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_tunnels_returns_all() {
    let tunnel = sample_tunnel();
    let state = build_state(MockTunnelService::with_tunnels(vec![tunnel]));
    let app = tunnel_router(state);

    let (status, json) = get_json(app, "/api/tunnels").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["tunnels"].as_array().unwrap().len(), 1);
    assert_eq!(json["tunnels"][0]["label"], "US West");
    assert_eq!(json["tunnels"][0]["country_code"], "US");
    assert_eq!(json["tunnels"][0]["status"], "down");
}

#[tokio::test]
async fn list_tunnels_returns_empty() {
    let state = build_state(MockTunnelService::empty());
    let app = tunnel_router(state);

    let (status, json) = get_json(app, "/api/tunnels").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["tunnels"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn list_tunnels_unauthorized_without_session() {
    let state = build_state(MockTunnelService::empty());
    let app = tunnel_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/tunnels")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// POST /api/tunnels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_tunnel_success() {
    let state = build_state(MockTunnelService::empty());
    let app = tunnel_router(state);

    let body = serde_json::json!({
        "label": "US West",
        "country_code": "US",
        "provider": "Mullvad",
        "config": "[Interface]\nPrivateKey = abc\n\n[Peer]\nPublicKey = xyz\nEndpoint = 1.2.3.4:51820\nAllowedIPs = 0.0.0.0/0\n"
    });

    let (status, json) = post_json(app, "/api/tunnels", &body.to_string()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["tunnel"]["label"], "US West");
    assert_eq!(json["tunnel"]["country_code"], "US");
    assert_eq!(json["message"], "tunnel imported successfully");
}

#[tokio::test]
async fn create_tunnel_without_provider() {
    let state = build_state(MockTunnelService::empty());
    let app = tunnel_router(state);

    let body = serde_json::json!({
        "label": "DE Berlin",
        "country_code": "DE",
        "config": "[Interface]\nPrivateKey = abc\n\n[Peer]\nPublicKey = xyz\nEndpoint = 5.6.7.8:51820\nAllowedIPs = 0.0.0.0/0\n"
    });

    let (status, json) = post_json(app, "/api/tunnels", &body.to_string()).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["tunnel"]["label"], "DE Berlin");
    assert!(json["tunnel"]["provider"].is_null());
}

#[tokio::test]
async fn create_tunnel_bad_json_returns_error() {
    let state = build_state(MockTunnelService::empty());
    let app = tunnel_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tunnels")
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 400 or 422 for deserialization failures depending on version.
    let status = resp.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

#[tokio::test]
async fn create_tunnel_missing_fields_returns_error() {
    let state = build_state(MockTunnelService::empty());
    let app = tunnel_router(state);

    // Missing required `config` field.
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tunnels")
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from(r#"{"label":"x","country_code":"US"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

#[tokio::test]
async fn create_tunnel_unauthorized_without_session() {
    let state = build_state(MockTunnelService::empty());
    let app = tunnel_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/tunnels")
                .header("Content-Type", "application/json")
                .extension(connect_info())
                .body(Body::from(
                    r#"{"label":"x","country_code":"US","config":"y"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// DELETE /api/tunnels/:id
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_tunnel_success() {
    let tunnel = sample_tunnel();
    let state = build_state(MockTunnelService::with_tunnels(vec![tunnel]));
    let app = tunnel_router(state);

    let (status, json) =
        delete_json(app, "/api/tunnels/00000000-0000-0000-0000-000000000010").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["message"], "tunnel deleted");
}

#[tokio::test]
async fn delete_tunnel_not_found() {
    let state = build_state(MockTunnelService::empty());
    let app = tunnel_router(state);

    let (status, json) =
        delete_json(app, "/api/tunnels/00000000-0000-0000-0000-000000000099").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn delete_tunnel_unauthorized_without_session() {
    let state = build_state(MockTunnelService::empty());
    let app = tunnel_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/tunnels/00000000-0000-0000-0000-000000000010")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
