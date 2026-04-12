//! Tests for the system status API endpoint (GET /api/system/status).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_types::api::SystemStatusResponse;

use crate::config::Config;
use crate::error::AppError;
use crate::service::auth::LoginResult;
use crate::service::{AuthService, SystemService};
use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpService, StubDiscoveryService, StubEventPublisher,
    StubProviderService, StubRoutingService, StubTunnelService,
};

// ---------------------------------------------------------------------------
// Mock services
// ---------------------------------------------------------------------------

/// Mock auth service that always validates the session (so admin routes pass).
struct AlwaysAuthService {
    admin_id: Uuid,
}

#[async_trait]
impl AuthService for AlwaysAuthService {
    async fn login(&self, _u: &str, _p: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(self.admin_id))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(self.admin_id))
    }
    async fn setup_admin(&self, _username: &str, _password: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        unimplemented!()
    }
}

/// Mock auth service that always rejects.
struct NeverAuthService;
#[async_trait]
impl AuthService for NeverAuthService {
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
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        unimplemented!()
    }
}

/// Mock system service returning a preconfigured response.
struct MockSystemService {
    response: Result<SystemStatusResponse, AppError>,
}

#[async_trait]
impl SystemService for MockSystemService {
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        match &self.response {
            Ok(r) => Ok(SystemStatusResponse {
                version: r.version.clone(),
                uptime_seconds: r.uptime_seconds,
                device_count: r.device_count,
                tunnel_count: r.tunnel_count,
                tunnel_active_count: r.tunnel_active_count,
                db_size_bytes: r.db_size_bytes,
                cpu_usage_percent: r.cpu_usage_percent,
                memory_used_bytes: r.memory_used_bytes,
                memory_total_bytes: r.memory_total_bytes,
            }),
            Err(_) => Err(AppError::Internal(anyhow::anyhow!("mock error"))),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state(auth: impl AuthService + 'static, system: impl SystemService + 'static) -> AppState {
    AppState::new(
        Arc::new(auth),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(system),
        Arc::new(StubTunnelService),
        Arc::new(StubEventPublisher),
        crate::log_broadcast::LogBroadcaster::new(16),
        crate::recent_errors::RecentErrors::new(),
        Config::default(),
        Instant::now(),
    )
}

fn system_app(state: AppState) -> Router {
    Router::new()
        .route("/api/system/status", get(crate::api::system::status))
        .with_state(state)
}

fn connect_info_ext() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn status_returns_200_with_correct_json() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(SystemStatusResponse {
                version: "1.2.3".to_owned(),
                uptime_seconds: 3600,
                device_count: 10,
                tunnel_count: 3,
                tunnel_active_count: 1,
                db_size_bytes: 4096,
                cpu_usage_percent: 25.5,
                memory_used_bytes: 1_073_741_824,
                memory_total_bytes: 4_294_967_296,
            }),
        },
    );

    let app = system_app(state);
    let req = Request::builder()
        .uri("/api/system/status")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["version"], "1.2.3");
    assert_eq!(json["uptime_seconds"], 3600);
    assert_eq!(json["device_count"], 10);
    assert_eq!(json["tunnel_count"], 3);
    assert_eq!(json["db_size_bytes"], 4096);
    assert_eq!(json["cpu_usage_percent"], 25.5);
    assert_eq!(json["memory_used_bytes"], 1_073_741_824_u64);
    assert_eq!(json["memory_total_bytes"], 4_294_967_296_u64);
}

#[tokio::test]
async fn status_requires_authentication() {
    let state = make_state(
        NeverAuthService,
        MockSystemService {
            response: Ok(SystemStatusResponse {
                version: "1.0.0".to_owned(),
                uptime_seconds: 0,
                device_count: 0,
                tunnel_count: 0,
                tunnel_active_count: 0,
                db_size_bytes: 0,
                cpu_usage_percent: 0.0,
                memory_used_bytes: 0,
                memory_total_bytes: 0,
            }),
        },
    );

    let app = system_app(state);
    let req = Request::builder()
        .uri("/api/system/status")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn status_service_error_returns_500() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Err(AppError::Internal(anyhow::anyhow!("db down"))),
        },
    );

    let app = system_app(state);
    let req = Request::builder()
        .uri("/api/system/status")
        .header("Authorization", "Bearer some-key")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn status_authenticates_via_bearer_token() {
    let admin_id = Uuid::new_v4();
    let state = make_state(
        AlwaysAuthService { admin_id },
        MockSystemService {
            response: Ok(SystemStatusResponse {
                version: "0.0.1".to_owned(),
                uptime_seconds: 1,
                device_count: 0,
                tunnel_count: 0,
                tunnel_active_count: 0,
                db_size_bytes: 0,
                cpu_usage_percent: 0.0,
                memory_used_bytes: 0,
                memory_total_bytes: 0,
            }),
        },
    );

    let app = system_app(state);
    let req = Request::builder()
        .uri("/api/system/status")
        .header("Authorization", "Bearer test-api-key")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
