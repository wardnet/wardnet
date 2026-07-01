//! Tests for the setup API endpoints (POST /api/setup, GET /api/setup/status).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{AdvanceWizardResponse, SetupStatusResponse, WizardMode, WizardStep};

use crate::state::AppState;
use crate::tests::stubs::{
    StubBackupService, StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService,
    StubDnsFilterService, StubDnsLocalService, StubDnsServer, StubDnsService, StubEventPublisher,
    StubLogService, StubNetworkZoneService, StubProviderService, StubRoutingService,
    StubSystemService, StubTunnelService,
};
use wardnetd_services::AuthService;
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::error::AppError;

// ---------------------------------------------------------------------------
// Mock auth service for setup tests
// ---------------------------------------------------------------------------

/// Mock auth service that tracks setup state.
struct MockSetupAuthService {
    setup_completed: bool,
    setup_result: Result<(), AppError>,
    wizard: Mutex<wardnetd_services::auth::service::WizardState>,
}

impl MockSetupAuthService {
    fn new(setup_completed: bool, setup_result: Result<(), AppError>) -> Self {
        let step = if setup_completed {
            WizardStep::Completed
        } else {
            WizardStep::Admin
        };
        Self {
            setup_completed,
            setup_result,
            wizard: Mutex::new(wardnetd_services::auth::service::WizardState { step, mode: None }),
        }
    }
}

#[async_trait]
impl AuthService for MockSetupAuthService {
    async fn login(&self, _u: &str, _p: &str, _remember_me: bool) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        // Authenticate any Bearer token to a stable admin id so the
        // setup_advance tests can drive the endpoint behind the
        // AdminAuth extractor without standing up a real session.
        Ok(Some(Uuid::nil()))
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
    async fn wizard_state(
        &self,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        Ok(*self.wizard.lock().unwrap())
    }
    async fn advance_wizard(
        &self,
        to_step: WizardStep,
        mode: Option<WizardMode>,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        let mut guard = self.wizard.lock().unwrap();
        guard.step = to_step;
        if mode.is_some() {
            guard.mode = mode;
        }
        Ok(*guard)
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state(auth: impl AuthService + 'static) -> AppState {
    AppState::new(
        Arc::new(auth),
        Arc::new(StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubRuleRequestService),
    )
}

fn setup_app(state: AppState) -> Router {
    Router::new()
        .route("/api/setup/status", get(crate::api::setup::setup_status))
        .route("/api/setup", post(crate::api::setup::setup))
        .route("/api/setup/advance", post(crate::api::setup::setup_advance))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn setup_status_returns_false_initially() {
    let state = make_state(MockSetupAuthService::new(false, Ok(())));
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
    let state = make_state(MockSetupAuthService::new(false, Ok(())));
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
    let state = make_state(MockSetupAuthService::new(
        true,
        Err(AppError::Conflict("setup already completed".to_owned())),
    ));
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
    let state = make_state(MockSetupAuthService::new(true, Ok(())));
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
    assert_eq!(json.wizard_step, WizardStep::Completed);
    assert!(json.wizard_mode.is_none());
}

#[tokio::test]
async fn setup_status_returns_admin_step_initially() {
    let state = make_state(MockSetupAuthService::new(false, Ok(())));
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
    assert_eq!(json.wizard_step, WizardStep::Admin);
    assert!(json.wizard_mode.is_none());
}

#[tokio::test]
async fn setup_advance_persists_step_and_mode() {
    let state = make_state(MockSetupAuthService::new(false, Ok(())));
    let app = setup_app(state);

    let body = serde_json::json!({
        "to_step": "dhcp",
        "wizard_mode": "primary",
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/setup/advance")
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test-key")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp_body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: AdvanceWizardResponse = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(json.wizard_step, WizardStep::Dhcp);
    assert_eq!(json.wizard_mode, Some(WizardMode::Primary));
}

#[tokio::test]
async fn setup_advance_rejects_unauthenticated() {
    let state = make_state(MockSetupAuthService::new(false, Ok(())));
    let app = setup_app(state);

    let body = serde_json::json!({
        "to_step": "dhcp",
        "wizard_mode": "primary",
    });

    // No Authorization header — AdminAuth should reject before the
    // service ever sees the call.
    let req = Request::builder()
        .method("POST")
        .uri("/api/setup/advance")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
