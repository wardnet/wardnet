//! Tests for the authentication API endpoints (POST /api/auth/login).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use tower::ServiceExt;
use uuid::Uuid;

use crate::config::Config;
use crate::error::AppError;
use crate::service::AuthService;
use crate::service::auth::LoginResult;
use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpService, StubDiscoveryService, StubEventPublisher,
    StubProviderService, StubRoutingService, StubSystemService, StubTunnelService,
};

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
        Arc::new(StubDhcpService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubEventPublisher),
        crate::log_broadcast::LogBroadcaster::new(16),
        crate::recent_errors::RecentErrors::new(),
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
