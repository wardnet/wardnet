//! Tests for the household-identity credential endpoints (ADR-0031, #1147).
//!
//! The OAuth callback gets the most attention here, because it is the one
//! unauthenticated route that *mints a session*. Three properties are worth a
//! test each and are easy to regress silently:
//!
//! 1. It dispatches on the **ceremony**, not the request — a sign-in sets a
//!    cookie, a link does not.
//! 2. `Location` is always one of two compile-time constants, so no
//!    caller-supplied text can reach the header.
//! 3. A failure redirects with a **stable code**, never `AppError`'s text.

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post, put};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{AuthMethodsResponse, OauthStartResponse, UserResponse};
use wardnet_common::auth::UserRole;
use wardnetd_data::repository::user_credential::CredentialSummary;
use wardnetd_services::error::AppError;
use wardnetd_services::user::{
    AuthMethods, EnrolmentInvite, EnrolmentSummary, NewUser, OauthOutcome, OauthProvider,
    OauthRedirect, ProviderStatus, ReturnTo, UserProfile, UserService,
};

use crate::api::tests::users::{
    make_state_with_user_service, stub_auth, stub_auth_failing_session_mint,
};

/// What a [`StubUserService`] should do when its callback is resolved.
#[derive(Clone)]
pub enum CallbackBehaviour {
    SignedIn {
        return_to: ReturnTo,
        remember_me: bool,
    },
    Linked {
        return_to: ReturnTo,
    },
    Fails(&'static str),
}

pub struct StubUserService {
    pub callback: CallbackBehaviour,
    /// What `require_authenticated()` reported *inside*
    /// `complete_oauth_callback` — i.e. whether the auth-context middleware had
    /// resolved a principal by the time the service ran. This is the property
    /// the link ceremony depends on and that no handler-level test can see.
    pub saw_authenticated: Mutex<Option<bool>>,
    /// Records `(state, code)` so a test can assert the handler forwarded the
    /// query parameters rather than inventing them.
    pub seen_callback: Mutex<Option<(String, String)>>,
    pub start_calls: Mutex<Vec<(OauthProvider, ReturnTo, bool)>>,
}

impl StubUserService {
    pub fn with_callback(callback: CallbackBehaviour) -> Self {
        Self {
            callback,
            saw_authenticated: Mutex::new(None),
            seen_callback: Mutex::new(None),
            start_calls: Mutex::new(Vec::new()),
        }
    }
}

fn profile(name: &str) -> UserProfile {
    UserProfile {
        id: Uuid::nil(),
        display_name: name.to_owned(),
        email: None,
        role: UserRole::Member,
        enabled: true,
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        updated_at: "2026-01-01T00:00:00Z".to_owned(),
    }
}

fn err_from(kind: &str) -> AppError {
    match kind {
        "unauthorized" => AppError::Unauthorized("unknown subject".to_owned()),
        "bad_request" => AppError::BadRequest("spent ceremony".to_owned()),
        "upstream" => AppError::UpstreamUnavailable("provider timed out".to_owned()),
        _ => AppError::Internal(anyhow::anyhow!("boom")),
    }
}

#[async_trait]
impl UserService for StubUserService {
    async fn list_users(&self) -> Result<Vec<UserProfile>, AppError> {
        Ok(vec![profile("Ana")])
    }
    async fn get_user(&self, _user_id: Uuid) -> Result<UserProfile, AppError> {
        Ok(profile("Ana"))
    }
    async fn create_user(&self, new_user: NewUser) -> Result<UserProfile, AppError> {
        Ok(profile(&new_user.display_name))
    }
    async fn update_profile(
        &self,
        _user_id: Uuid,
        display_name: &str,
        _email: Option<&str>,
    ) -> Result<UserProfile, AppError> {
        Ok(profile(display_name))
    }
    async fn set_enabled(&self, _user_id: Uuid, _enabled: bool) -> Result<UserProfile, AppError> {
        Ok(profile("Ana"))
    }
    async fn set_role(&self, _user_id: Uuid, _role: UserRole) -> Result<UserProfile, AppError> {
        Ok(profile("Ana"))
    }
    async fn delete_user(&self, _user_id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn list_credentials(&self, user_id: Uuid) -> Result<Vec<CredentialSummary>, AppError> {
        use wardnetd_data::repository::user_credential::CredentialKind;
        Ok(vec![
            CredentialSummary {
                id: "cred-1".to_owned(),
                user_id: user_id.to_string(),
                kind: CredentialKind::Password,
                subject: "ana".to_owned(),
                label: None,
                metadata: "{}".to_owned(),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
                last_used_at: None,
            },
            CredentialSummary {
                id: "cred-2".to_owned(),
                user_id: user_id.to_string(),
                kind: CredentialKind::Github,
                subject: "12345".to_owned(),
                label: Some("ana-on-github".to_owned()),
                metadata: "{}".to_owned(),
                created_at: "2026-01-02T00:00:00Z".to_owned(),
                last_used_at: Some("2026-01-03T00:00:00Z".to_owned()),
            },
            CredentialSummary {
                id: "cred-3".to_owned(),
                user_id: user_id.to_string(),
                kind: CredentialKind::Google,
                subject: "ana@example.invalid".to_owned(),
                label: None,
                metadata: "{}".to_owned(),
                created_at: "2026-01-02T00:00:00Z".to_owned(),
                last_used_at: None,
            },
            // No code path produces a passkey yet (#1194), but the column
            // accepts one — so the mapper must already render it as itself
            // rather than falling through to a kind that *is* implemented.
            CredentialSummary {
                id: "cred-4".to_owned(),
                user_id: user_id.to_string(),
                kind: CredentialKind::Passkey,
                subject: "opaque-credential-id".to_owned(),
                label: None,
                metadata: "{}".to_owned(),
                created_at: "2026-01-02T00:00:00Z".to_owned(),
                last_used_at: None,
            },
        ])
    }
    async fn change_own_password(&self, _current: &str, _new: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn issue_enrolment(&self, user_id: Uuid) -> Result<EnrolmentInvite, AppError> {
        Ok(EnrolmentInvite {
            token: "one-time-token".to_owned(),
            expires_at: "2026-01-04T00:00:00Z".to_owned(),
            user_id,
        })
    }
    async fn list_enrolments(&self, user_id: Uuid) -> Result<Vec<EnrolmentSummary>, AppError> {
        Ok(vec![
            EnrolmentSummary {
                id: Uuid::nil(),
                user_id,
                created_at: "2026-01-01T00:00:00Z".to_owned(),
                expires_at: "2026-01-04T00:00:00Z".to_owned(),
                used_at: None,
            },
            EnrolmentSummary {
                id: Uuid::nil(),
                user_id,
                created_at: "2025-12-01T00:00:00Z".to_owned(),
                expires_at: "2025-12-04T00:00:00Z".to_owned(),
                used_at: Some("2025-12-02T00:00:00Z".to_owned()),
            },
        ])
    }
    async fn revoke_enrolment(&self, _user_id: Uuid, _enrolment_id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn redeem_enrolment(
        &self,
        token: &str,
        _password: &str,
    ) -> Result<UserProfile, AppError> {
        if token == "good-token" {
            Ok(profile("Bruno"))
        } else {
            Err(AppError::Unauthorized(
                "that invitation is not valid".to_owned(),
            ))
        }
    }
    async fn cleanup_expired_enrolments(&self) -> Result<u64, AppError> {
        Ok(0)
    }
    async fn available_methods(&self) -> Result<AuthMethods, AppError> {
        Ok(AuthMethods {
            password: true,
            providers: vec![ProviderStatus {
                provider: OauthProvider::Google,
                client_id: Some("client-id".to_owned()),
                enabled: false,
                // Stored *on*, effective *off* — the exact divergence a
                // settings form must not write back (it would silently
                // disable a provider whose secret is merely missing).
                enabled_stored: true,
                configured: false,
                redirect_uri: Some(
                    "https://home.example/api/auth/oauth/google/callback".to_owned(),
                ),
            }],
        })
    }
    async fn configure_oauth_provider(
        &self,
        provider: OauthProvider,
        client_id: &str,
        _client_secret: Option<&str>,
        enabled: bool,
    ) -> Result<ProviderStatus, AppError> {
        Ok(ProviderStatus {
            provider,
            client_id: Some(client_id.to_owned()),
            enabled,
            enabled_stored: enabled,
            configured: true,
            redirect_uri: None,
        })
    }
    async fn list_oauth_providers(&self) -> Result<Vec<ProviderStatus>, AppError> {
        Ok(self.available_methods().await?.providers)
    }
    async fn clear_oauth_provider(&self, _provider: OauthProvider) -> Result<(), AppError> {
        Ok(())
    }
    async fn start_oauth(
        &self,
        provider: OauthProvider,
        return_to: ReturnTo,
        remember_me: bool,
    ) -> Result<OauthRedirect, AppError> {
        self.start_calls
            .lock()
            .unwrap()
            .push((provider, return_to, remember_me));
        Ok(OauthRedirect {
            url: "https://accounts.google.com/o/oauth2/v2/auth?x=1".to_owned(),
        })
    }
    async fn complete_oauth_callback(
        &self,
        state: &str,
        code: &str,
    ) -> Result<OauthOutcome, AppError> {
        *self.seen_callback.lock().unwrap() = Some((state.to_owned(), code.to_owned()));
        *self.saw_authenticated.lock().unwrap() =
            Some(wardnetd_services::auth_context::require_authenticated().is_ok());
        match &self.callback {
            CallbackBehaviour::SignedIn {
                return_to,
                remember_me,
            } => Ok(OauthOutcome::SignedIn {
                profile: profile("Ana"),
                role: UserRole::Admin,
                provider: OauthProvider::Google,
                return_to: *return_to,
                remember_me: *remember_me,
            }),
            CallbackBehaviour::Linked { return_to } => Ok(OauthOutcome::Linked {
                user_id: Uuid::nil(),
                provider: OauthProvider::Google,
                return_to: *return_to,
            }),
            CallbackBehaviour::Fails(kind) => Err(err_from(kind)),
        }
    }
    async fn unlink_oauth(
        &self,
        _user_id: Uuid,
        _provider: OauthProvider,
    ) -> Result<u64, AppError> {
        Ok(1)
    }
}

fn app(user: Arc<dyn UserService>) -> Router {
    app_with(user, false)
}

/// [`app`], optionally with an `AuthService` whose session mint always fails.
fn app_with(user: Arc<dyn UserService>, fail_session_mint: bool) -> Router {
    let state = if fail_session_mint {
        make_state_with_user_service(stub_auth_failing_session_mint(), user)
    } else {
        make_state_with_user_service(stub_auth(), user)
    };
    Router::new()
        .route(
            "/api/auth/methods",
            get(crate::api::user_auth::available_methods),
        )
        .route(
            "/api/auth/oauth/{provider}/start",
            get(crate::api::user_auth::start_oauth),
        )
        .route(
            "/api/auth/oauth/{provider}/callback",
            get(crate::api::user_auth::complete_oauth_callback),
        )
        .route(
            "/api/auth/enrolments/redeem",
            post(crate::api::user_auth::redeem_enrolment),
        )
        .route(
            "/api/auth/providers",
            get(crate::api::user_auth::list_oauth_providers),
        )
        .route(
            "/api/auth/providers/{provider}",
            put(crate::api::user_auth::configure_oauth_provider)
                .delete(crate::api::user_auth::clear_oauth_provider),
        )
        .with_state(state)
}

fn get_req(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn location(resp: &axum::response::Response) -> String {
    resp.headers()
        .get(axum::http::header::LOCATION)
        .expect("a redirect must carry Location")
        .to_str()
        .unwrap()
        .to_owned()
}

fn set_cookie(resp: &axum::response::Response) -> Option<String> {
    resp.headers()
        .get(axum::http::header::SET_COOKIE)
        .map(|v| v.to_str().unwrap().to_owned())
}

// ── GET /api/auth/methods ────────────────────────────────────────────────────

#[tokio::test]
async fn methods_is_reachable_without_a_session() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let resp = app(svc)
        .oneshot(get_req("/api/auth/methods"))
        .await
        .unwrap();

    // No Authorization header at all: this backs the sign-in surface, whose
    // whole job is to be reachable before anybody has a session.
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: AuthMethodsResponse = serde_json::from_slice(&body).unwrap();
    assert!(json.password, "the local password is the floor");
    assert_eq!(json.providers.len(), 1);
}

// ── GET /api/auth/oauth/{provider}/start ─────────────────────────────────────

#[tokio::test]
async fn start_returns_json_and_parks_the_caller_intent_on_the_ceremony() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let resp = app(svc.clone())
        .oneshot(get_req(
            "/api/auth/oauth/google/start?return_to=admin_app&remember_me=true",
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: OauthStartResponse = serde_json::from_slice(&body).unwrap();
    assert!(json.url.starts_with("https://accounts.google.com/"));

    // `return_to` and `remember_me` cannot survive a provider redirect on
    // their own, so they must reach the ceremony here or not at all.
    let calls = svc.start_calls.lock().unwrap();
    assert_eq!(
        calls.as_slice(),
        &[(OauthProvider::Google, ReturnTo::AdminApp, true)]
    );
}

#[tokio::test]
async fn start_defaults_return_to_and_remember_me() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let resp = app(svc.clone())
        .oneshot(get_req("/api/auth/oauth/google/start"))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let calls = svc.start_calls.lock().unwrap();
    // `remember_me` defaults to *false*: it gates session refresh, so the
    // safe default is the short-lived session.
    assert_eq!(
        calls.as_slice(),
        &[(OauthProvider::Google, ReturnTo::Admin, false)]
    );
}

#[tokio::test]
async fn start_rejects_an_unknown_provider() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let resp = app(svc.clone())
        .oneshot(get_req("/api/auth/oauth/facebook/start"))
        .await
        .unwrap();

    // A 400, not a fallback to a configured provider: guessing would run a
    // ceremony against the wrong identity provider.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(svc.start_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn start_rejects_an_open_redirect_shaped_return_to() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    for hostile in ["//evil.com", "/\\evil.com", "https://evil.com", "admin/"] {
        let resp = app(svc.clone())
            .oneshot(get_req(&format!(
                "/api/auth/oauth/google/start?return_to={hostile}"
            )))
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "{hostile} must be rejected, never sanitised into something close enough"
        );
    }
    assert!(svc.start_calls.lock().unwrap().is_empty());
}

// ── GET /api/auth/oauth/{provider}/callback ──────────────────────────────────

#[tokio::test]
async fn callback_signs_in_and_sets_a_session_cookie() {
    let svc = Arc::new(StubUserService::with_callback(
        CallbackBehaviour::SignedIn {
            return_to: ReturnTo::Admin,
            remember_me: false,
        },
    ));
    let resp = app(svc.clone())
        .oneshot(get_req(
            "/api/auth/oauth/google/callback?state=abc&code=xyz",
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&resp), "/admin/");

    let cookie = set_cookie(&resp).expect("a sign-in must set the session cookie");
    assert!(cookie.starts_with("wardnet_session="));
    // This ceremony did not ask to be remembered, so it gets the short expiry.
    assert!(cookie.contains("Max-Age=3600"), "{cookie}");
    // Shared with `POST /api/auth/login` rather than rebuilt here — a second
    // cookie with weaker attributes is exactly the drift that sharing prevents.
    assert!(cookie.contains("HttpOnly"), "{cookie}");
    // `Lax`, not `Strict`. The callback is a cross-site top-level navigation
    // from the provider, and `Strict` would withhold the cookie on exactly
    // that — making the *link* ceremony permanently impossible while leaving
    // sign-in (which sets rather than sends) working, so nothing would notice.
    assert!(cookie.contains("SameSite=Lax"), "{cookie}");

    // The handler forwards the provider's parameters verbatim.
    assert_eq!(
        *svc.seen_callback.lock().unwrap(),
        Some(("abc".to_owned(), "xyz".to_owned()))
    );
}

#[tokio::test]
async fn callback_returns_the_pwa_to_the_pwa() {
    let svc = Arc::new(StubUserService::with_callback(
        CallbackBehaviour::SignedIn {
            return_to: ReturnTo::AdminApp,
            remember_me: true,
        },
    ));
    let resp = app(svc)
        .oneshot(get_req(
            "/api/auth/oauth/google/callback?state=abc&code=xyz",
        ))
        .await
        .unwrap();

    // The ceremony remembered where it began; the bare provider redirect
    // could not have said.
    assert_eq!(location(&resp), "/admin-app/");

    // …and it remembered `remember_me`, which reaches the minted session or
    // nowhere: there is deliberately no endpoint that raises it afterwards,
    // since that would upgrade any short session into a 90-day one.
    let cookie = set_cookie(&resp).expect("a sign-in sets the session cookie");
    assert!(
        cookie.contains("Max-Age=7776000"),
        "remember_me must reach the session: {cookie}"
    );
}

#[tokio::test]
async fn callback_links_without_minting_a_session() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Linked {
        return_to: ReturnTo::Admin,
    }));
    let resp = app(svc)
        .oneshot(get_req(
            "/api/auth/oauth/google/callback?state=abc&code=xyz",
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&resp), "/admin/");
    // The caller already had a session. Re-minting one on a link would
    // silently re-negotiate its lifetime.
    assert_eq!(set_cookie(&resp), None);
}

#[tokio::test]
async fn callback_maps_failures_to_stable_codes_and_never_reflects_the_error() {
    for (kind, expected) in [
        ("unauthorized", "access_denied"),
        ("bad_request", "invalid_request"),
        ("upstream", "provider_unavailable"),
        ("internal", "server_error"),
    ] {
        let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
            kind,
        )));
        let resp = app(svc)
            .oneshot(get_req(
                "/api/auth/oauth/google/callback?state=abc&code=xyz",
            ))
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let loc = location(&resp);
        assert_eq!(loc, format!("/admin/?oauth_error={expected}"));
        // The service's message must not travel in the URL: it can name a
        // provider, a user, or an internal failure.
        for leaked in ["unknown subject", "spent ceremony", "timed out", "boom"] {
            assert!(!loc.contains(leaked), "{loc} leaked {leaked}");
        }
        assert_eq!(set_cookie(&resp), None, "a failed sign-in sets no cookie");
    }
}

#[tokio::test]
async fn callback_handles_a_provider_refusal_without_resolving_a_ceremony() {
    let svc = Arc::new(StubUserService::with_callback(
        CallbackBehaviour::SignedIn {
            return_to: ReturnTo::Admin,
            remember_me: false,
        },
    ));
    let resp = app(svc.clone())
        .oneshot(get_req(
            "/api/auth/oauth/google/callback?error=access_denied",
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&resp), "/admin/?oauth_error=access_denied");
    // Nothing to resolve: the ceremony must be left to expire unused rather
    // than spent on a request that carried no code.
    assert_eq!(*svc.seen_callback.lock().unwrap(), None);
}

#[tokio::test]
async fn callback_without_state_or_code_does_not_consume_the_ceremony() {
    for uri in [
        "/api/auth/oauth/google/callback",
        "/api/auth/oauth/google/callback?state=abc",
        "/api/auth/oauth/google/callback?code=xyz",
    ] {
        let svc = Arc::new(StubUserService::with_callback(
            CallbackBehaviour::SignedIn {
                return_to: ReturnTo::Admin,
                remember_me: false,
            },
        ));
        let resp = app(svc.clone()).oneshot(get_req(uri)).await.unwrap();

        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(location(&resp), "/admin/?oauth_error=invalid_request");
        assert_eq!(*svc.seen_callback.lock().unwrap(), None, "{uri}");
    }
}

#[tokio::test]
async fn callback_rejects_an_unknown_provider_with_a_status_not_a_redirect() {
    let svc = Arc::new(StubUserService::with_callback(
        CallbackBehaviour::SignedIn {
            return_to: ReturnTo::Admin,
            remember_me: false,
        },
    ));
    let resp = app(svc.clone())
        .oneshot(get_req(
            "/api/auth/oauth/facebook/callback?state=abc&code=xyz",
        ))
        .await
        .unwrap();

    // Nothing was consumed and a URL this wrong was never a real ceremony,
    // so there is nobody to return anywhere.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(*svc.seen_callback.lock().unwrap(), None);
}

// ── POST /api/auth/enrolments/redeem ─────────────────────────────────────────

#[tokio::test]
async fn redeem_is_unauthenticated_and_returns_the_enrolled_user() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/enrolments/redeem")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"{"token":"good-token","password":"a-fine-password"}"#,
        ))
        .unwrap();

    let resp = app(svc).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: UserResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.display_name, "Bruno");
}

#[tokio::test]
async fn redeem_refuses_an_invalid_token() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/enrolments/redeem")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"{"token":"spent-token","password":"a-fine-password"}"#,
        ))
        .unwrap();

    let resp = app(svc).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── provider configuration (admin) ───────────────────────────────────────────

#[tokio::test]
async fn configure_provider_requires_a_session() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let req = Request::builder()
        .method("PUT")
        .uri("/api/auth/providers/google")
        .header("Content-Type", "application/json")
        .body(Body::from(r#"{"client_id":"id","enabled":true}"#))
        .unwrap();

    // No Authorization header: unlike its neighbours in this file, this route
    // is admin-only.
    let resp = app(svc).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn configure_provider_echoes_the_new_status() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let req = Request::builder()
        .method("PUT")
        .uri("/api/auth/providers/google")
        .header("Authorization", "Bearer test-key")
        .header("Content-Type", "application/json")
        .body(Body::from(
            r#"{"client_id":"the-id","client_secret":"s3cr3t","enabled":true}"#,
        ))
        .unwrap();

    let resp = app(svc).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("\"client_id\":\"the-id\""), "{text}");
    // The secret is never echoed back — `configured` is the whole of what a
    // reader learns about it.
    assert!(!text.contains("s3cr3t"), "{text}");
}

#[tokio::test]
async fn clear_provider_returns_no_content() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/auth/providers/github")
        .header("Authorization", "Bearer test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app(svc).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn provider_routes_reject_an_unknown_provider() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let req = Request::builder()
        .method("DELETE")
        .uri("/api/auth/providers/facebook")
        .header("Authorization", "Bearer test-key")
        .body(Body::empty())
        .unwrap();

    let resp = app(svc).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_failed_session_mint_still_answers_303() {
    // The ceremony is already spent by this point, so bailing out with a JSON
    // error would strand the person on a raw error body in the address bar
    // with no way back. Every failure in this handler owes them a redirect.
    let svc = Arc::new(StubUserService::with_callback(
        CallbackBehaviour::SignedIn {
            return_to: ReturnTo::AdminApp,
            remember_me: false,
        },
    ));
    let resp = app_with(svc, true)
        .oneshot(get_req(
            "/api/auth/oauth/google/callback?state=abc&code=xyz",
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    // Back to the surface the ceremony began on, with a stable code — and no
    // cookie, because no session exists.
    assert_eq!(location(&resp), "/admin-app/?oauth_error=server_error");
    assert_eq!(set_cookie(&resp), None);
}

#[tokio::test]
async fn provider_config_is_admin_only_and_carries_the_client_id() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));

    // Unauthenticated: refused. This is the whole reason the endpoint exists
    // separately from `/api/auth/methods` — the client id names the
    // household's own OAuth application.
    let resp = app(svc.clone())
        .oneshot(get_req("/api/auth/providers"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/providers")
        .header("Authorization", "Bearer test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app(svc).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("\"client_id\":\"client-id\""), "{text}");
    // The *stored* flag, not the effective one. The stub reports stored `true`
    // with effective `false`; a form that round-tripped the effective value
    // would silently switch the provider off.
    assert!(text.contains("\"enabled\":true"), "{text}");
}

#[tokio::test]
async fn public_methods_never_carry_the_client_id() {
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Fails(
        "unauthorized",
    )));
    let resp = app(svc)
        .oneshot(get_req("/api/auth/methods"))
        .await
        .unwrap();

    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        !text.contains("client_id"),
        "the unauthenticated sign-in contract must disclose no client id: {text}"
    );
    // …and it reports the *effective* availability, which the stub sets false.
    assert!(text.contains("\"enabled\":false"), "{text}");
}

// ── The callback inside the real middleware chain ─────────────────────────────
//
// Everything above drives handlers through a bare `Router::new().route(...)`,
// which is fine for status codes and bodies but has **no auth-context
// middleware**. That blind spot is exactly what let a `SameSite=Strict`
// session cookie ship: the link ceremony needs a resolved principal at the
// callback, and no handler-level test could observe whether one existed.
//
// These two build the router the way `api::mod::router` does — `AuthContextLayer`
// over `resolve_auth_context` — and assert on what the *service* saw.

use axum::middleware::from_fn_with_state;
use wardnetd_services::auth_context::AuthContextLayer;

/// The callback route with the production auth-context chain layered on.
fn app_with_auth_middleware(user: Arc<dyn UserService>) -> Router {
    let state = make_state_with_user_service(stub_auth(), user);
    Router::new()
        .route(
            "/api/auth/oauth/{provider}/callback",
            get(crate::api::user_auth::complete_oauth_callback),
        )
        .layer(AuthContextLayer)
        .layer(from_fn_with_state(
            state.clone(),
            crate::api::middleware::resolve_auth_context,
        ))
        .with_state(state)
}

#[tokio::test]
async fn callback_resolves_a_principal_when_the_request_carries_a_credential() {
    // The load-bearing fact behind account linking: a credential arriving on
    // the callback *does* become an authenticated context by the time
    // `complete_oauth_callback` runs. If it did not — as with a `Strict`
    // session cookie, which the browser withholds on this cross-site
    // navigation — the link arm's `require_authenticated()` would fail and the
    // ceremony would already be spent.
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Linked {
        return_to: ReturnTo::Admin,
    }));

    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/oauth/google/callback?state=abc&code=xyz")
        .header("Authorization", "Bearer test-key")
        .body(Body::empty())
        .unwrap();
    let resp = app_with_auth_middleware(svc.clone())
        .oneshot(req)
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(location(&resp), "/admin/");
    assert_eq!(
        *svc.saw_authenticated.lock().unwrap(),
        Some(true),
        "the callback must run with a resolved principal when one was presented"
    );
}

#[tokio::test]
async fn callback_is_anonymous_when_the_request_carries_no_credential() {
    // The other half of the contract, and the reason the cookie's `SameSite`
    // value matters: with nothing presented the callback runs anonymously, so
    // a *link* cannot complete. A sign-in still can — which is why breaking
    // linking this way is silent.
    let svc = Arc::new(StubUserService::with_callback(CallbackBehaviour::Linked {
        return_to: ReturnTo::Admin,
    }));

    let resp = app_with_auth_middleware(svc.clone())
        .oneshot(get_req(
            "/api/auth/oauth/google/callback?state=abc&code=xyz",
        ))
        .await
        .unwrap();

    // The stub links regardless, so the redirect still succeeds — what is
    // asserted here is the *context*, which is what the real service gates on.
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        *svc.saw_authenticated.lock().unwrap(),
        Some(false),
        "no credential must mean no principal — the link arm relies on this"
    );
}
