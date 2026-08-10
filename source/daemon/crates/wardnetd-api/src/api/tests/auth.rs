//! Tests for the authentication API endpoints (POST /api/auth/login).

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use tower::ServiceExt;

use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService, StubDnsFilterService,
    StubDnsLocalService, StubDnsServer, StubDnsService, StubEventPublisher, StubLogService,
    StubNetworkZoneService, StubProviderService, StubRoutingService, StubSystemService,
    StubTunnelService,
};
use wardnetd_services::AuthService;
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::error::AppError;
use wardnetd_services::auth::{CurrentUser, LoginAttempt};
use wardnet_common::auth::{AuthenticatedUser, UserRole};
use wardnet_test_support::principal;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Mock auth services
// ---------------------------------------------------------------------------

/// Mock auth service returning a configurable login result or error.
struct MockAuthService {
    login_result: Result<LoginResult, AppError>,
}

#[async_trait]
impl AuthService for MockAuthService {
    async fn current_user(&self) -> Result<CurrentUser, AppError> {
        Ok(CurrentUser {
            user_id: Uuid::nil(),
            display_name: "admin".to_owned(),
            email: None,
            role: UserRole::Admin,
        })
    }
    async fn login(&self, _attempt: LoginAttempt<'_>) -> Result<LoginResult, AppError> {
        match &self.login_result {
            Ok(r) => Ok(LoginResult {
                token: r.token.clone(),
                max_age_seconds: r.max_age_seconds,
            }),
            Err(_) => Err(AppError::Unauthorized("invalid credentials".to_owned())),
        }
    }

    async fn validate_session(&self, _token: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(None)
    }

    async fn validate_api_key(&self, _key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(None)
    }

    async fn setup_admin(&self, _username: &str, _password: &str) -> Result<(), AppError> {
        unimplemented!()
    }

    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        unimplemented!()
    }
    async fn wizard_state(
        &self,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        unimplemented!()
    }
    async fn advance_wizard(
        &self,
        _to_step: wardnet_common::api::WizardStep,
        _mode: Option<wardnet_common::api::WizardMode>,
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

/// Mock auth service for refresh endpoint tests.
struct MockRefreshAuthService {
    /// `Ok(())` → returns a successful `LoginResult`; `Err(())` → returns Unauthorized.
    refresh_result: Result<(), ()>,
}

#[async_trait]
impl AuthService for MockRefreshAuthService {
    async fn current_user(&self) -> Result<CurrentUser, AppError> {
        Ok(CurrentUser {
            user_id: Uuid::nil(),
            display_name: "admin".to_owned(),
            email: None,
            role: UserRole::Admin,
        })
    }
    async fn login(&self, _attempt: LoginAttempt<'_>) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        // Always return a valid admin so the SessionAuth extractor passes.
        Ok(Some(principal::admin(Uuid::nil())))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(None)
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
        unimplemented!()
    }
    async fn advance_wizard(
        &self,
        _to_step: wardnet_common::api::WizardStep,
        _mode: Option<wardnet_common::api::WizardMode>,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        unimplemented!()
    }
    async fn logout_session(&self, _token: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        match self.refresh_result {
            Ok(()) => Ok(LoginResult {
                token: "same-token".to_owned(),
                max_age_seconds: 720 * 3600,
            }),
            Err(()) => Err(AppError::Unauthorized(
                "session not found or not refreshable".to_owned(),
            )),
        }
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

/// Stateful auth service for logout tests: tracks valid session tokens (and
/// optionally API keys) in memory so the login → logout → replay sequence can
/// be driven end-to-end through the real handlers and extractors.
struct InMemorySessionAuthService {
    tokens: Mutex<HashSet<String>>,
    api_keys: Mutex<HashSet<String>>,
}

impl InMemorySessionAuthService {
    fn new() -> Self {
        Self {
            tokens: Mutex::new(HashSet::new()),
            api_keys: Mutex::new(HashSet::new()),
        }
    }
}

#[async_trait]
impl AuthService for InMemorySessionAuthService {
    async fn current_user(&self) -> Result<CurrentUser, AppError> {
        Ok(CurrentUser {
            user_id: Uuid::nil(),
            display_name: "admin".to_owned(),
            email: None,
            role: UserRole::Admin,
        })
    }
    async fn login(&self, _attempt: LoginAttempt<'_>) -> Result<LoginResult, AppError> {
        let token = "integration-session-token".to_owned();
        self.tokens.lock().unwrap().insert(token.clone());
        Ok(LoginResult {
            token,
            max_age_seconds: 86400,
        })
    }
    async fn validate_session(&self, token: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(self
            .tokens
            .lock()
            .unwrap()
            .contains(token)
            .then(|| principal::admin(Uuid::nil())))
    }
    async fn validate_api_key(&self, key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(self
            .api_keys
            .lock()
            .unwrap()
            .contains(key)
            .then(|| principal::admin(Uuid::nil())))
    }
    async fn logout_session(&self, token: &str) -> Result<(), AppError> {
        self.tokens.lock().unwrap().remove(token);
        Ok(())
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
        unimplemented!()
    }
    async fn advance_wizard(
        &self,
        _to_step: wardnet_common::api::WizardStep,
        _mode: Option<wardnet_common::api::WizardMode>,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_state(auth: impl AuthService + 'static) -> AppState {
    make_state_from_arc(Arc::new(auth))
}

fn make_state_from_arc(auth: Arc<dyn AuthService>) -> AppState {
    AppState::new(
        auth,
        Arc::new(crate::tests::stubs::StubBackupService),
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
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    )
}

fn login_app(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/login", post(crate::api::auth::login))
        .with_state(state)
}

fn refresh_app(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/refresh", post(crate::api::auth::refresh))
        .with_state(state)
}

fn auth_app(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/login", post(crate::api::auth::login))
        .route("/api/auth/logout", post(crate::api::auth::logout))
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
async fn login_over_plain_http_omits_secure_attribute() {
    // The plain-HTTP `:7411` surface has no `SecureTransport` marker, so the
    // session cookie must NOT carry `Secure` — browsers drop `Secure` cookies
    // over `http://`, which would strand the session and break login.
    let state = make_state(MockAuthService {
        login_result: Ok(LoginResult {
            token: "plain-http-token".to_owned(),
            max_age_seconds: 86400,
        }),
    });

    let app = login_app(state);
    let body = serde_json::json!({ "username": "admin", "password": "password123" });

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let cookie = resp
        .headers()
        .get("set-cookie")
        .expect("expected Set-Cookie header")
        .to_str()
        .unwrap();

    assert!(cookie.contains("wardnet_session=plain-http-token"));
    assert!(cookie.contains("HttpOnly"));
    assert!(
        !cookie.contains("Secure"),
        "plain-HTTP cookie must not be Secure, got: {cookie}"
    );
}

#[tokio::test]
async fn login_over_tls_includes_secure_attribute() {
    // With the `SecureTransport` marker (present only on the `:443` TLS app),
    // the cookie must carry `Secure`.
    let state = make_state(MockAuthService {
        login_result: Ok(LoginResult {
            token: "tls-token".to_owned(),
            max_age_seconds: 86400,
        }),
    });

    let app = login_app(state);
    let body = serde_json::json!({ "username": "admin", "password": "password123" });

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .extension(crate::api::middleware::SecureTransport)
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let cookie = resp
        .headers()
        .get("set-cookie")
        .expect("expected Set-Cookie header")
        .to_str()
        .unwrap();

    assert!(cookie.contains("wardnet_session=tls-token"));
    assert!(
        cookie.contains("Secure"),
        "TLS cookie must be Secure, got: {cookie}"
    );
}

#[tokio::test]
async fn refresh_over_plain_http_omits_secure_attribute() {
    let state = make_state(MockRefreshAuthService {
        refresh_result: Ok(()),
    });
    let app = refresh_app(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let cookie = resp
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie expected")
        .to_str()
        .unwrap();
    assert!(
        !cookie.contains("Secure"),
        "plain-HTTP refresh cookie must not be Secure, got: {cookie}"
    );
}

#[tokio::test]
async fn refresh_over_tls_includes_secure_attribute() {
    let state = make_state(MockRefreshAuthService {
        refresh_result: Ok(()),
    });
    let app = refresh_app(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .extension(crate::api::middleware::SecureTransport)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let cookie = resp
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie expected")
        .to_str()
        .unwrap();
    assert!(
        cookie.contains("Secure"),
        "TLS refresh cookie must be Secure, got: {cookie}"
    );
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

#[tokio::test]
async fn refresh_success_returns_204_and_set_cookie() {
    let state = make_state(MockRefreshAuthService {
        refresh_result: Ok(()),
    });
    let app = refresh_app(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .header("Cookie", "wardnet_session=valid-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let cookie = resp
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie expected")
        .to_str()
        .unwrap();
    assert!(cookie.contains("wardnet_session=same-token"));
    assert!(cookie.contains("Max-Age=2592000")); // 720 * 3600
}

#[tokio::test]
async fn refresh_via_bearer_token_returns_204() {
    // Covers the bearer-token branch of SessionAuth::session_token extraction.
    let state = make_state(MockRefreshAuthService {
        refresh_result: Ok(()),
    });
    let app = refresh_app(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .header("Authorization", "Bearer valid-bearer-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(resp.headers().contains_key("set-cookie"));
}

#[tokio::test]
async fn logout_returns_204_and_clears_the_session_cookie() {
    let auth = Arc::new(InMemorySessionAuthService::new());
    auth.tokens.lock().unwrap().insert("live-token".to_owned());
    let app = auth_app(make_state_from_arc(auth));

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("Cookie", "wardnet_session=live-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let cookie = resp
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie expected")
        .to_str()
        .unwrap();
    assert!(
        cookie.starts_with("wardnet_session=;"),
        "cookie value must be emptied, got: {cookie}"
    );
    assert!(cookie.contains("Max-Age=0"), "got: {cookie}");
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("Path=/"));
    assert!(
        !cookie.contains("Secure"),
        "plain-HTTP clear cookie must not be Secure, got: {cookie}"
    );
}

#[tokio::test]
async fn logout_over_tls_clears_cookie_with_secure_attribute() {
    let auth = Arc::new(InMemorySessionAuthService::new());
    auth.tokens.lock().unwrap().insert("tls-token".to_owned());
    let app = auth_app(make_state_from_arc(auth));

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("Cookie", "wardnet_session=tls-token")
        .extension(connect_info_ext())
        .extension(crate::api::middleware::SecureTransport)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let cookie = resp
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie expected")
        .to_str()
        .unwrap();
    assert!(cookie.contains("Max-Age=0"), "got: {cookie}");
    assert!(
        cookie.contains("Secure"),
        "TLS clear cookie must be Secure, got: {cookie}"
    );
}

#[tokio::test]
async fn logout_via_bearer_token_returns_204() {
    let auth = Arc::new(InMemorySessionAuthService::new());
    auth.tokens
        .lock()
        .unwrap()
        .insert("bearer-token".to_owned());
    let app = auth_app(make_state_from_arc(auth.clone()));

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("Authorization", "Bearer bearer-token")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(auth.tokens.lock().unwrap().is_empty());
}

#[tokio::test]
async fn logout_revokes_the_authenticating_bearer_session_not_a_stale_cookie() {
    // A request can carry a stale cookie alongside the valid bearer token
    // that actually authenticates it. Logout must revoke the bearer session
    // — deleting the stale cookie's (nonexistent) session and reporting 204
    // would leave the live session valid while claiming it was revoked.
    let auth = Arc::new(InMemorySessionAuthService::new());
    auth.tokens.lock().unwrap().insert("live-bearer".to_owned());
    let app = auth_app(make_state_from_arc(auth.clone()));

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("Cookie", "wardnet_session=stale-cookie-token")
        .header("Authorization", "Bearer live-bearer")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(
        auth.tokens.lock().unwrap().is_empty(),
        "the bearer session that authenticated the request must be revoked"
    );
}

#[tokio::test]
async fn logout_via_api_key_returns_401_without_touching_sessions() {
    // API keys authenticate but are not sessions: there is nothing to log
    // out, so the handler must refuse rather than report a bogus 204.
    let auth = Arc::new(InMemorySessionAuthService::new());
    auth.api_keys
        .lock()
        .unwrap()
        .insert("my-api-key".to_owned());
    auth.tokens
        .lock()
        .unwrap()
        .insert("someone-elses-session".to_owned());
    let app = auth_app(make_state_from_arc(auth.clone()));

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("Authorization", "Bearer my-api-key")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    // The API key still works and no session was collateral damage.
    assert!(auth.api_keys.lock().unwrap().contains("my-api-key"));
    assert_eq!(auth.tokens.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn logout_without_session_returns_401() {
    let auth = Arc::new(InMemorySessionAuthService::new());
    let app = auth_app(make_state_from_arc(auth));

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// End-to-end session-revocation flow through the real handlers and the
/// `SessionAuth` extractor: login issues a cookie, logout revokes it server-side,
/// and replaying the old cookie afterwards is rejected with 401.
#[tokio::test]
async fn login_then_logout_then_old_cookie_is_rejected() {
    let auth = Arc::new(InMemorySessionAuthService::new());
    let app = auth_app(make_state_from_arc(auth.clone()));

    // 1. Login → session cookie issued, session exists server-side.
    let body = serde_json::json!({ "username": "admin", "password": "password123" });
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("Content-Type", "application/json")
        .extension(connect_info_ext())
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie_header = "wardnet_session=integration-session-token";
    assert_eq!(auth.tokens.lock().unwrap().len(), 1);

    // 2. Logout with that cookie → 204, session removed from the store.
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("Cookie", cookie_header)
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert!(
        auth.tokens.lock().unwrap().is_empty(),
        "session must be deleted server-side"
    );

    // 3. Replaying the old cookie is rejected by the auth extractor.
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("Cookie", cookie_header)
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_without_session_returns_401() {
    // MockAuthService.validate_session returns Ok(None) → SessionAuth extractor rejects.
    let state = make_state(MockAuthService {
        login_result: Ok(LoginResult {
            token: "unused".to_owned(),
            max_age_seconds: 0,
        }),
    });
    let app = refresh_app(state);

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/refresh")
        .extension(connect_info_ext())
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
