//! Tests for the user-identity endpoint (GET /api/users/me).

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use tower::ServiceExt;
use wardnet_common::api::{MeResponse, WizardMode, WizardStep};

use crate::state::AppState;
use crate::tests::stubs::{
    StubBackupService, StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService,
    StubDnsFilterService, StubDnsLocalService, StubDnsServer, StubDnsService, StubEventPublisher,
    StubLogService, StubNetworkZoneService, StubProviderService, StubRoutingService,
    StubSystemService, StubTunnelService,
};
use uuid::Uuid;
use wardnet_common::auth::{AuthenticatedUser, UserRole};
use wardnet_test_support::principal;
use wardnetd_services::AuthService;
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::auth::{CurrentUser, LoginAttempt};
use wardnetd_services::error::AppError;

/// Mock auth service that authenticates any Bearer token and returns a
/// configurable `current_user` result.
struct MockUsersAuthService {
    /// `Ok` yields a user with this display name; `Err` simulates the account
    /// having been deleted out from under a live session.
    username: Result<String, ()>,
}

#[async_trait]
impl AuthService for MockUsersAuthService {
    async fn current_user(&self) -> Result<CurrentUser, AppError> {
        match &self.username {
            Ok(name) => Ok(CurrentUser {
                user_id: Uuid::nil(),
                display_name: name.clone(),
                email: None,
                role: UserRole::Admin,
            }),
            Err(()) => Err(AppError::Unauthorized(
                "user account no longer exists".to_owned(),
            )),
        }
    }
    async fn login(&self, _attempt: LoginAttempt<'_>) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(None)
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        // Authenticate any Bearer token so the tests can drive the
        // SessionAuth extractor without a real session.
        Ok(Some(principal::admin(Uuid::nil())))
    }
    async fn setup_admin(&self, _u: &str, _p: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        Ok(true)
    }
    async fn wizard_state(
        &self,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        Ok(wardnetd_services::auth::service::WizardState {
            step: WizardStep::Completed,
            mode: None,
        })
    }
    async fn advance_wizard(
        &self,
        _to_step: WizardStep,
        _mode: Option<WizardMode>,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        unimplemented!()
    }
    async fn logout_session(&self, _token: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

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
        Arc::new(crate::tests::stubs::StubAccessRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    )
}

fn users_app(state: AppState) -> Router {
    Router::new()
        .route("/api/users/me", get(crate::api::users::me))
        .with_state(state)
}

#[tokio::test]
async fn me_returns_the_admin_username() {
    let state = make_state(MockUsersAuthService {
        username: Ok("operator".to_owned()),
    });
    let app = users_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/users/me")
        .header("Authorization", "Bearer test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: MeResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.username, "operator");
}

#[tokio::test]
async fn me_rejects_unauthenticated() {
    let state = make_state(MockUsersAuthService {
        username: Ok("operator".to_owned()),
    });
    let app = users_app(state);

    // No Authorization header — SessionAuth rejects before the service
    // ever sees the call.
    let req = Request::builder()
        .method("GET")
        .uri("/api/users/me")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_surfaces_service_errors() {
    let state = make_state(MockUsersAuthService { username: Err(()) });
    let app = users_app(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/users/me")
        .header("Authorization", "Bearer test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
