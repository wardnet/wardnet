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
pub struct MockUsersAuthService {
    /// `Ok` yields a user with this display name; `Err` simulates the account
    /// having been deleted out from under a live session.
    username: Result<String, ()>,
    /// When true, `issue_verified_session` fails — standing in for a locked or
    /// full database at the moment a federated sign-in tries to mint a session.
    pub fail_session_mint: bool,
}

#[async_trait]
impl AuthService for MockUsersAuthService {
    async fn issue_verified_session(
        &self,
        _user_id: uuid::Uuid,
        remember_me: bool,
        _user_agent: Option<&str>,
    ) -> Result<LoginResult, AppError> {
        if self.fail_session_mint {
            return Err(AppError::Internal(anyhow::anyhow!("database is locked")));
        }
        // The OAuth callback's sanctioned call: a credential has just been
        // verified by `UserService`, and session policy lives here. The
        // lifetime tracks `remember_me` so a test can tell the two apart.
        Ok(LoginResult {
            token: "federated-session-token".to_owned(),
            max_age_seconds: if remember_me { 7_776_000 } else { 3_600 },
        })
    }
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

/// A `MockUsersAuthService` that authenticates any Bearer token — enough to
/// drive the `SessionAuth` extractor without a real session.
pub fn stub_auth() -> MockUsersAuthService {
    MockUsersAuthService {
        username: Ok("operator".to_owned()),
        fail_session_mint: false,
    }
}

/// [`stub_auth`], but every attempt to mint a verified session fails.
pub fn stub_auth_failing_session_mint() -> MockUsersAuthService {
    MockUsersAuthService {
        username: Ok("operator".to_owned()),
        fail_session_mint: true,
    }
}

/// [`make_state`] plus an injected [`UserService`], for the household-identity
/// routes. Shared with `api::tests::user_auth`.
pub fn make_state_with_user_service(
    auth: impl AuthService + 'static,
    user: Arc<dyn wardnetd_services::UserService>,
) -> AppState {
    make_state(auth).with_user_service(user)
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
        fail_session_mint: false,
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
        fail_session_mint: false,
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
    let state = make_state(MockUsersAuthService {
        username: Err(()),
        fail_session_mint: false,
    });
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

// ── The household directory (ADR-0031, #1147) ────────────────────────────────
//
// These drive the routes through the real router, so the assertions cover the
// handler's own work: path parsing, status-code choice, and the response
// shape. Authorization itself lives in `UserService` (`.agents/auth.md`), so
// what is checked here is that a handler cannot be reached *without* a
// session, not that it re-implements the guard.

use crate::api::tests::user_auth::{CallbackBehaviour, StubUserService};
use axum::routing::{delete, post, put};
use wardnet_common::api::{
    CredentialKindDto, EnrolmentInviteResponse, ListEnrolmentsResponse,
    ListUserCredentialsResponse, ListUsersResponse, UserResponse,
};

fn directory_app() -> Router {
    let user = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let state = make_state_with_user_service(stub_auth(), user);
    Router::new()
        .route(
            "/api/users",
            get(crate::api::users::list_users).post(crate::api::users::create_user),
        )
        .route(
            "/api/users/{id}",
            get(crate::api::users::get_user)
                .patch(crate::api::users::update_profile)
                .delete(crate::api::users::delete_user),
        )
        .route(
            "/api/users/{id}/enabled",
            put(crate::api::users::set_enabled),
        )
        .route("/api/users/{id}/role", put(crate::api::users::set_role))
        .route(
            "/api/users/{id}/credentials",
            get(crate::api::users::list_credentials),
        )
        .route(
            "/api/users/{id}/credentials/{provider}",
            delete(crate::api::users::unlink_oauth),
        )
        .route(
            "/api/users/{id}/enrolments",
            get(crate::api::users::list_enrolments).post(crate::api::users::issue_enrolment),
        )
        .route(
            "/api/users/{id}/enrolments/{enrolment_id}",
            delete(crate::api::users::revoke_enrolment),
        )
        .route(
            "/api/users/me/password",
            post(crate::api::users::change_own_password),
        )
        .with_state(state)
}

const UID: &str = "00000000-0000-0000-0000-000000000001";

fn authed(method: &str, uri: &str, body: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", "Bearer test-key");
    if body.is_some() {
        builder = builder.header("Content-Type", "application/json");
    }
    builder
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_owned())))
        .unwrap()
}

#[tokio::test]
async fn directory_routes_require_a_session() {
    // Every one of these is admin-gated. Without a credential the extractor
    // rejects before `UserService` is reached at all.
    for (method, uri) in [
        ("GET", "/api/users"),
        ("POST", "/api/users"),
        ("GET", "/api/users/{id}"),
        ("PATCH", "/api/users/{id}"),
        ("DELETE", "/api/users/{id}"),
        ("PUT", "/api/users/{id}/enabled"),
        ("PUT", "/api/users/{id}/role"),
        ("GET", "/api/users/{id}/credentials"),
        ("GET", "/api/users/{id}/enrolments"),
        ("POST", "/api/users/{id}/enrolments"),
        ("POST", "/api/users/me/password"),
    ] {
        let uri = uri.replace("{id}", UID);
        let req = Request::builder()
            .method(method)
            .uri(&uri)
            .header("Content-Type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = directory_app().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must not be reachable without a session"
        );
    }
}

#[tokio::test]
async fn list_users_returns_the_directory() {
    let resp = directory_app()
        .oneshot(authed("GET", "/api/users", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: ListUsersResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.users.len(), 1);
    assert_eq!(json.users[0].display_name, "Ana");
}

#[tokio::test]
async fn create_user_passes_the_body_through() {
    let resp = directory_app()
        .oneshot(authed(
            "POST",
            "/api/users",
            Some(r#"{"display_name":"Cleo","email":null,"role":"member"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: UserResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.display_name, "Cleo");
}

#[tokio::test]
async fn update_profile_passes_the_new_name_through() {
    let resp = directory_app()
        .oneshot(authed(
            "PATCH",
            &format!("/api/users/{UID}"),
            Some(r#"{"display_name":"Renamed","email":null}"#),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: UserResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.display_name, "Renamed");
}

#[tokio::test]
async fn mutations_that_return_nothing_answer_204() {
    for (method, uri, body) in [
        ("DELETE", format!("/api/users/{UID}"), None),
        (
            "DELETE",
            format!("/api/users/{UID}/credentials/google"),
            None,
        ),
        ("DELETE", format!("/api/users/{UID}/enrolments/{UID}"), None),
        (
            "POST",
            "/api/users/me/password".to_owned(),
            Some(r#"{"current_password":"old","new_password":"new"}"#),
        ),
    ] {
        let resp = directory_app()
            .oneshot(authed(method, &uri, body))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT, "{method} {uri}");
    }
}

#[tokio::test]
async fn enabled_and_role_return_the_updated_user() {
    for (uri, body) in [
        (format!("/api/users/{UID}/enabled"), r#"{"enabled":false}"#),
        (format!("/api/users/{UID}/role"), r#"{"role":"admin"}"#),
    ] {
        let resp = directory_app()
            .oneshot(authed("PUT", &uri, Some(body)))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{uri}");
        let bytes = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
        let _: UserResponse = serde_json::from_slice(&bytes).unwrap();
    }
}

#[tokio::test]
async fn credential_and_enrolment_lists_are_wrapped() {
    let resp = directory_app()
        .oneshot(authed(
            "GET",
            &format!("/api/users/{UID}/credentials"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let creds: ListUserCredentialsResponse = serde_json::from_slice(&bytes).unwrap();
    // The kind is mapped through an explicit match, not a string round-trip,
    // so each variant must render as itself. A `_ =>` arm there would let a
    // new credential kind quietly appear as an existing one.
    let kinds: Vec<_> = creds.credentials.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        vec![
            CredentialKindDto::Password,
            CredentialKindDto::Github,
            CredentialKindDto::Google,
            CredentialKindDto::Passkey,
        ],
        "every credential kind must render as itself"
    );
    // The summary carries `metadata` and `user_id`; the wire form drops both,
    // and neither may reappear by accident.
    let raw = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!raw.contains("metadata"), "{raw}");
    assert_eq!(creds.credentials[1].label.as_deref(), Some("ana-on-github"));
    assert_eq!(
        creds.credentials[1].last_used_at.as_deref(),
        Some("2026-01-03T00:00:00Z")
    );

    let resp = directory_app()
        .oneshot(authed("GET", &format!("/api/users/{UID}/enrolments"), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let enrolments: ListEnrolmentsResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(enrolments.enrolments.len(), 2);
    // `used_at` is what distinguishes an open invitation from a spent one —
    // a spent row is kept rather than deleted so the UI can say when.
    assert_eq!(enrolments.enrolments[0].used_at, None);
    assert_eq!(
        enrolments.enrolments[1].used_at.as_deref(),
        Some("2025-12-02T00:00:00Z")
    );
}

#[tokio::test]
async fn issue_enrolment_returns_the_one_time_token() {
    let resp = directory_app()
        .oneshot(authed(
            "POST",
            &format!("/api/users/{UID}/enrolments"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let invite: EnrolmentInviteResponse = serde_json::from_slice(&bytes).unwrap();
    // The token is present exactly once, in this response — the admin UI has
    // no second chance to fetch it.
    assert_eq!(invite.token, "one-time-token");
}

#[tokio::test]
async fn a_malformed_id_is_a_400_not_a_500() {
    for uri in [
        "/api/users/not-a-uuid".to_owned(),
        "/api/users/not-a-uuid/credentials".to_owned(),
        "/api/users/not-a-uuid/enrolments".to_owned(),
        format!("/api/users/not-a-uuid/enrolments/{UID}"),
        format!("/api/users/{UID}/enrolments/not-a-uuid"),
    ] {
        let resp = directory_app()
            .oneshot(authed("GET", &uri, None))
            .await
            .unwrap();
        // GET is not defined on every path above; what matters is that the
        // ones that exist reject the id rather than panicking on parse.
        assert!(
            resp.status() == StatusCode::BAD_REQUEST
                || resp.status() == StatusCode::METHOD_NOT_ALLOWED,
            "{uri} gave {}",
            resp.status()
        );
    }
}

#[tokio::test]
async fn unlink_rejects_an_unknown_provider() {
    let resp = directory_app()
        .oneshot(authed(
            "DELETE",
            &format!("/api/users/{UID}/credentials/facebook"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_user_returns_one_user() {
    let resp = directory_app()
        .oneshot(authed("GET", &format!("/api/users/{UID}"), None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: UserResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json.display_name, "Ana");
    // The profile type structurally has no credential field, so there is
    // nothing here that could leak one — assert the wire form agrees.
    let raw = String::from_utf8(bytes.to_vec()).unwrap();
    for secret in ["password", "hash", "secret", "token"] {
        assert!(!raw.contains(secret), "{raw} leaked {secret}");
    }
}
