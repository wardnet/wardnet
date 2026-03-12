//! Tests for the setup API endpoints (POST /api/setup, GET /api/setup/status).

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_types::api::SetupStatusResponse;

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
        Arc::new(StubDhcpService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
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
