//! OAuth sign-in tests. No network: `wiremock` stands in for the provider, and
//! the endpoint URLs are struct fields precisely so it can.

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;
use wardnet_common::auth::UserRole;
use wardnet_test_support::principal;
use wardnetd_data::repository::session::SessionRepository;
use wardnetd_data::repository::user::UserRepository;
use wardnetd_data::repository::user_credential::{
    CredentialKind, CredentialRow, UserCredentialRepository,
};
use wardnetd_data::repository::user_enrolment::UserEnrolmentRepository;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::auth_context;
use crate::ddns::{KEY_DOMAIN, KEY_PROVIDER, KEY_SUBDOMAIN, PROVIDER_CLOUDFLARE, PROVIDER_WARDNET};
use crate::error::AppError;
use crate::tests::repo_mocks::{
    MockCredentialRepo, MockEnrolmentRepo, MockSecretStore, MockSessionRepo, MockSystemConfigRepo,
    MockUserRepo, user_row,
};
use crate::user::oauth::{OauthConfig, OauthProvider, ProviderEndpoints, ReqwestOauthClient};
use crate::user::{UserService, UserServiceImpl};

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const MEMBER_ID: &str = "00000000-0000-0000-0000-000000000002";
const FQDN: &str = "happy-einstein.wardnet.app";

fn admin() -> Uuid {
    Uuid::parse_str(ADMIN_ID).unwrap()
}
fn member() -> Uuid {
    Uuid::parse_str(MEMBER_ID).unwrap()
}

struct Fixture {
    svc: UserServiceImpl,
    credentials: Arc<MockCredentialRepo>,
    config: Arc<MockSystemConfigRepo>,
    secrets: Arc<MockSecretStore>,
}

/// A household with an admin and a member, and a canonical public hostname.
///
/// `google` points at `mock_base` so the exchange is testable; `GitHub` keeps its
/// production endpoints, which no test here calls.
fn fixture(mock_base: Option<&str>) -> Fixture {
    let users = Arc::new(MockUserRepo::with_rows(vec![
        user_row(ADMIN_ID, "admin", UserRole::Admin, true),
        user_row(MEMBER_ID, "kid", UserRole::Member, true),
    ]));
    let credentials = Arc::new(MockCredentialRepo::joined_to(Arc::clone(&users)));
    let enrolments = Arc::new(MockEnrolmentRepo::empty());
    let sessions = Arc::new(MockSessionRepo::joined_to(Arc::clone(&users)));
    let config = Arc::new(MockSystemConfigRepo::empty());
    let secrets = Arc::new(MockSecretStore::empty());

    // A public hostname, or federated sign-in is correctly unavailable.
    config
        .values
        .lock()
        .unwrap()
        .insert(KEY_PROVIDER.to_owned(), PROVIDER_WARDNET.to_owned());
    config
        .values
        .lock()
        .unwrap()
        .insert(KEY_SUBDOMAIN.to_owned(), FQDN.to_owned());

    let mut svc = UserServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::clone(&credentials) as Arc<dyn UserCredentialRepository>,
        Arc::clone(&enrolments) as Arc<dyn UserEnrolmentRepository>,
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        OauthConfig {
            system_config: Arc::clone(&config)
                as Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
            secrets: Arc::clone(&secrets) as Arc<dyn wardnetd_data::secret_store::SecretStore>,
        },
        Arc::new(ReqwestOauthClient::new().unwrap()),
    );

    if let Some(base) = mock_base {
        svc = svc.with_endpoints(
            OauthProvider::Google,
            ProviderEndpoints {
                authorize_url: format!("{base}/authorize"),
                token_url: format!("{base}/token"),
                userinfo_url: format!("{base}/userinfo"),
                scopes: "openid email".to_owned(),
            },
        );
    }

    Fixture {
        svc,
        credentials,
        config,
        secrets,
    }
}

/// Configure Google as an admin would.
async fn configure_google(f: &Fixture) {
    let ctx = principal::admin_context(admin());
    auth_context::with_context(
        ctx,
        f.svc.configure_oauth_provider(
            OauthProvider::Google,
            "client-id",
            Some("client-secret"),
            true,
        ),
    )
    .await
    .unwrap();
}

/// Pull the `state` out of an authorize URL.
fn state_from(url: &str) -> String {
    reqwest::Url::parse(url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.into_owned())
        .expect("the authorize url must carry a state")
}

// -- configuration --------------------------------------------------------

#[tokio::test]
async fn configuring_a_provider_stores_the_secret_out_of_reach() {
    let f = fixture(None);
    configure_google(&f).await;

    // The client id is ordinary config; the secret is in the vault.
    assert_eq!(
        f.config
            .read(&OauthProvider::Google.client_id_key())
            .as_deref(),
        Some("client-id")
    );
    assert!(
        f.secrets.contains(&OauthProvider::Google.secret_path()),
        "the client secret must be stored in the SecretStore"
    );
    assert!(
        !f.config
            .values
            .lock()
            .unwrap()
            .values()
            .any(|v| v.contains("client-secret")),
        "the client secret must never land in system_config"
    );
}

#[tokio::test]
async fn provider_status_never_returns_the_secret() {
    let f = fixture(None);
    configure_google(&f).await;

    let methods = f.svc.available_methods().await.unwrap();
    let google = methods
        .providers
        .iter()
        .find(|p| p.provider == OauthProvider::Google)
        .unwrap();

    assert!(google.enabled);
    // `configured` is the only thing said about the secret.
    assert!(google.configured);
    assert_eq!(
        google.redirect_uri.as_deref(),
        Some("https://happy-einstein.wardnet.app/api/auth/oauth/google/callback"),
        "the admin needs the exact URI to paste into the provider"
    );
}

#[tokio::test]
async fn the_local_password_is_always_offered() {
    // The floor. There is no configuration that turns it off, because a box
    // whose only credential needed a reachable provider would be unreachable
    // during a WAN outage.
    let f = fixture(None);
    let methods = f.svc.available_methods().await.unwrap();
    assert!(methods.password);
    assert!(
        methods.providers.iter().all(|p| !p.enabled),
        "nothing is configured yet, so no provider should be offered"
    );
}

#[tokio::test]
async fn a_provider_is_not_enabled_until_it_is_fully_configured() {
    // A half-configured provider would render a button that always fails.
    let f = fixture(None);
    let ctx = principal::admin_context(admin());

    // Enabled and given an id, but no secret.
    auth_context::with_context(
        ctx,
        f.svc
            .configure_oauth_provider(OauthProvider::Google, "client-id", None, true),
    )
    .await
    .unwrap();

    let methods = f.svc.available_methods().await.unwrap();
    let google = methods
        .providers
        .iter()
        .find(|p| p.provider == OauthProvider::Google)
        .unwrap();
    assert!(!google.enabled, "no secret means not usable");
    assert!(!google.configured);
}

#[tokio::test]
async fn federated_login_is_unavailable_without_a_canonical_hostname() {
    // A provider cannot reach a redirect URI on a box with no public hostname,
    // so the honest answer is "unavailable", with no redirect URI to show.
    let f = fixture(None);
    f.config.values.lock().unwrap().remove(KEY_SUBDOMAIN);
    configure_google(&f).await;

    let methods = f.svc.available_methods().await.unwrap();
    let google = methods
        .providers
        .iter()
        .find(|p| p.provider == OauthProvider::Google)
        .unwrap();
    assert!(!google.enabled);
    assert!(google.redirect_uri.is_none());

    // And starting a ceremony refuses rather than building a broken URL.
    let result = f.svc.start_oauth(OauthProvider::Google).await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn configuring_a_provider_requires_admin() {
    let f = fixture(None);
    let ctx = principal::member_context(member());

    let result = auth_context::with_context(
        ctx,
        f.svc
            .configure_oauth_provider(OauthProvider::Google, "id", Some("secret"), true),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn a_later_configure_can_omit_the_secret_without_losing_it() {
    // So an admin can flip `enabled` or fix a typo'd id without re-pasting a
    // secret they may no longer have.
    let f = fixture(None);
    configure_google(&f).await;
    let ctx = principal::admin_context(admin());

    let status = auth_context::with_context(
        ctx,
        f.svc
            .configure_oauth_provider(OauthProvider::Google, "new-client-id", None, true),
    )
    .await
    .unwrap();

    assert!(status.configured, "the existing secret must survive");
    assert_eq!(
        f.config
            .read(&OauthProvider::Google.client_id_key())
            .as_deref(),
        Some("new-client-id")
    );
}

#[tokio::test]
async fn clearing_a_provider_removes_its_secret() {
    let f = fixture(None);
    configure_google(&f).await;
    let ctx = principal::admin_context(admin());

    auth_context::with_context(ctx, f.svc.clear_oauth_provider(OauthProvider::Google))
        .await
        .unwrap();

    assert!(!f.secrets.contains(&OauthProvider::Google.secret_path()));
    let methods = f.svc.available_methods().await.unwrap();
    assert!(methods.providers.iter().all(|p| !p.enabled));
}

// -- the authorize step ---------------------------------------------------

#[tokio::test]
async fn start_oauth_builds_a_pkce_authorize_url() {
    let f = fixture(None);
    configure_google(&f).await;

    let redirect = f.svc.start_oauth(OauthProvider::Google).await.unwrap();
    let url = reqwest::Url::parse(&redirect.url).unwrap();
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();

    assert_eq!(
        params.get("response_type").map(String::as_str),
        Some("code")
    );
    assert_eq!(
        params.get("client_id").map(String::as_str),
        Some("client-id")
    );
    assert_eq!(
        params.get("code_challenge_method").map(String::as_str),
        Some("S256"),
        "PKCE binds the code to this ceremony; plain must never be used"
    );
    assert!(params.contains_key("code_challenge"));
    assert!(params.contains_key("state"));
    assert_eq!(
        params.get("redirect_uri").map(String::as_str),
        Some("https://happy-einstein.wardnet.app/api/auth/oauth/google/callback")
    );
}

#[tokio::test]
async fn each_ceremony_gets_a_fresh_state_and_challenge() {
    let f = fixture(None);
    configure_google(&f).await;

    let a = f.svc.start_oauth(OauthProvider::Google).await.unwrap();
    let b = f.svc.start_oauth(OauthProvider::Google).await.unwrap();

    assert_ne!(
        state_from(&a.url),
        state_from(&b.url),
        "reusing a state across ceremonies would make one replayable in the other"
    );
}

// -- the callback ---------------------------------------------------------

/// Stand up a provider that issues a token and reports `sub`.
async fn google_mock(subject: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "provider-access-token",
            "token_type": "Bearer",
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/userinfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sub": subject,
            "email": "person@example.com",
        })))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_linked_provider_account_signs_in() {
    let server = google_mock("google-subject-1").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;

    // An admin has linked this provider account to the member.
    f.credentials.rows.lock().unwrap().push(CredentialRow {
        id: "cred-google".to_owned(),
        user_id: MEMBER_ID.to_owned(),
        kind: CredentialKind::Google,
        subject: "google-subject-1".to_owned(),
        secret: None,
        label: Some("person@example.com".to_owned()),
        metadata: "{}".to_owned(),
        created_at: "2026-01-01T00:00:00+00:00".to_owned(),
        last_used_at: None,
    });

    let redirect = f.svc.start_oauth(OauthProvider::Google).await.unwrap();
    let state = state_from(&redirect.url);

    let (profile, role) = f
        .svc
        .complete_oauth(&state, "provider-auth-code")
        .await
        .expect("a linked account must sign in");

    assert_eq!(profile.id, member());
    assert_eq!(role, UserRole::Member, "the role comes from the users row");
    assert!(
        f.credentials.last_used("cred-google").is_some(),
        "a successful sign-in stamps the credential"
    );
}

#[tokio::test]
async fn an_unknown_provider_subject_is_refused_and_creates_nothing() {
    // The rule that keeps a home network from being joinable by anyone with a
    // Google account: Wardnet never auto-creates a user from a federated login.
    let server = google_mock("nobody-has-linked-this").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;

    let redirect = f.svc.start_oauth(OauthProvider::Google).await.unwrap();
    let state = state_from(&redirect.url);

    let result = f.svc.complete_oauth(&state, "provider-auth-code").await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
    assert!(
        f.credentials.rows.lock().unwrap().is_empty(),
        "no credential may be created by an unrecognised federated login"
    );
}

#[tokio::test]
async fn a_mismatched_state_is_refused() {
    let server = google_mock("google-subject-1").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;
    let _ = f.svc.start_oauth(OauthProvider::Google).await.unwrap();

    let result = f
        .svc
        .complete_oauth("a-state-we-never-issued", "provider-auth-code")
        .await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
}

#[tokio::test]
async fn a_state_is_single_use() {
    // The ceremony entry is taken, not read, so a replayed callback fails even
    // though the first one succeeded.
    let server = google_mock("google-subject-1").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;
    f.credentials.rows.lock().unwrap().push(CredentialRow {
        id: "cred-google".to_owned(),
        user_id: MEMBER_ID.to_owned(),
        kind: CredentialKind::Google,
        subject: "google-subject-1".to_owned(),
        secret: None,
        label: None,
        metadata: "{}".to_owned(),
        created_at: "2026-01-01T00:00:00+00:00".to_owned(),
        last_used_at: None,
    });

    let redirect = f.svc.start_oauth(OauthProvider::Google).await.unwrap();
    let state = state_from(&redirect.url);

    f.svc.complete_oauth(&state, "code").await.unwrap();
    let replay = f.svc.complete_oauth(&state, "code").await;
    assert!(
        matches!(replay, Err(AppError::Unauthorized(_))),
        "a replayed state must be refused"
    );
}

#[tokio::test]
async fn a_provider_500_surfaces_as_upstream_unavailable() {
    // Not `Unauthorized`: the caller's credentials were not rejected, the
    // provider broke. Reporting 401 would send a household hunting for a
    // password problem during somebody else's outage.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;
    let redirect = f.svc.start_oauth(OauthProvider::Google).await.unwrap();
    let state = state_from(&redirect.url);

    let result = f.svc.complete_oauth(&state, "code").await;
    assert!(matches!(result, Err(AppError::UpstreamUnavailable(_))));
}

#[tokio::test]
async fn a_hanging_provider_is_given_up_on() {
    // The 10s ceiling exists so a dead WAN cannot pin a worker. Asserted with a
    // delay past it, so a regression that drops the timeout hangs this test
    // rather than passing quietly.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(30))
                .set_body_json(serde_json::json!({ "access_token": "t" })),
        )
        .mount(&server)
        .await;

    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;
    let redirect = f.svc.start_oauth(OauthProvider::Google).await.unwrap();
    let state = state_from(&redirect.url);

    let started = std::time::Instant::now();
    let result = f.svc.complete_oauth(&state, "code").await;
    let elapsed = started.elapsed();

    assert!(matches!(result, Err(AppError::UpstreamUnavailable(_))));
    assert!(
        elapsed < Duration::from_secs(25),
        "the request must be abandoned at the timeout, not wait for the provider ({elapsed:?})"
    );
}

#[tokio::test]
async fn completing_a_ceremony_for_an_unconfigured_provider_is_refused() {
    // Configuration can be cleared mid-ceremony; the callback must not proceed
    // with a half-removed provider.
    let server = google_mock("google-subject-1").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;
    let redirect = f.svc.start_oauth(OauthProvider::Google).await.unwrap();
    let state = state_from(&redirect.url);

    let ctx = principal::admin_context(admin());
    auth_context::with_context(ctx, f.svc.clear_oauth_provider(OauthProvider::Google))
        .await
        .unwrap();

    let result = f.svc.complete_oauth(&state, "code").await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

// -- linking and unlinking ------------------------------------------------

#[tokio::test]
async fn link_oauth_attaches_the_provider_account_to_the_caller() {
    let server = google_mock("google-subject-9").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;

    // The ceremony must be started by the same user that links it — see
    // `a_link_ceremony_cannot_be_redeemed_by_a_different_user`.
    let ctx = principal::member_context(member());
    let redirect =
        auth_context::with_context(ctx.clone(), f.svc.start_oauth(OauthProvider::Google))
            .await
            .unwrap();
    let state = state_from(&redirect.url);

    auth_context::with_context(ctx.clone(), f.svc.link_oauth(&state, "code"))
        .await
        .unwrap();

    let credentials = auth_context::with_context(ctx, f.svc.list_credentials(member()))
        .await
        .unwrap();
    assert_eq!(credentials.len(), 1);
    assert_eq!(credentials[0].kind, CredentialKind::Google);
    assert_eq!(credentials[0].subject, "google-subject-9");
}

#[tokio::test]
async fn link_oauth_requires_authentication() {
    let server = google_mock("google-subject-9").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;
    let redirect = f.svc.start_oauth(OauthProvider::Google).await.unwrap();
    let state = state_from(&redirect.url);

    let result = f.svc.link_oauth(&state, "code").await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn a_provider_account_cannot_be_linked_to_two_users() {
    // The `(kind, subject)` uniqueness constraint is the anti-hijack invariant.
    let server = google_mock("google-subject-9").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;

    // The member already holds this link.
    f.credentials.rows.lock().unwrap().push(CredentialRow {
        id: "cred-existing".to_owned(),
        user_id: MEMBER_ID.to_owned(),
        kind: CredentialKind::Google,
        subject: "google-subject-9".to_owned(),
        secret: None,
        label: None,
        metadata: "{}".to_owned(),
        created_at: "2026-01-01T00:00:00+00:00".to_owned(),
        last_used_at: None,
    });
    f.credentials.enforce_subject_uniqueness();

    let ctx = principal::admin_context(admin());
    let redirect =
        auth_context::with_context(ctx.clone(), f.svc.start_oauth(OauthProvider::Google))
            .await
            .unwrap();
    let state = state_from(&redirect.url);

    let result = auth_context::with_context(ctx, f.svc.link_oauth(&state, "code")).await;
    match result {
        Err(AppError::Conflict(message)) => {
            assert!(
                !message.contains(MEMBER_ID) && !message.contains("kid"),
                "the refusal must not disclose who holds the link: {message}"
            );
        }
        other => panic!("expected a Conflict, got {other:?}"),
    }
}

#[tokio::test]
async fn unlink_oauth_removes_only_that_provider() {
    let f = fixture(None);
    let member_id = MEMBER_ID.to_owned();
    // A password and a Google link.
    f.credentials
        .set_password(
            "cred-pw",
            &member_id,
            "kid@example.com",
            "$argon2-hash",
            "now",
        )
        .await
        .unwrap();
    f.credentials.rows.lock().unwrap().push(CredentialRow {
        id: "cred-google".to_owned(),
        user_id: member_id.clone(),
        kind: CredentialKind::Google,
        subject: "google-subject-1".to_owned(),
        secret: None,
        label: None,
        metadata: "{}".to_owned(),
        created_at: "2026-01-01T00:00:00+00:00".to_owned(),
        last_used_at: None,
    });
    let ctx = principal::member_context(member());

    let removed = auth_context::with_context(
        ctx.clone(),
        f.svc.unlink_oauth(member(), OauthProvider::Google),
    )
    .await
    .unwrap();
    assert_eq!(removed, 1);

    // The password — the floor — is untouched.
    assert!(
        f.credentials
            .find_password(&member_id)
            .await
            .unwrap()
            .is_some(),
        "unlinking a provider must never remove the local password"
    );
}

#[tokio::test]
async fn a_member_cannot_unlink_another_users_provider() {
    let f = fixture(None);
    let ctx = principal::member_context(member());

    let result =
        auth_context::with_context(ctx, f.svc.unlink_oauth(admin(), OauthProvider::Google)).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// -- regressions from the high-effort code review --------------------------

#[tokio::test]
async fn a_link_ceremony_cannot_be_redeemed_by_a_different_user() {
    // The takeover path: an attacker starts a ceremony, consents with their own
    // provider account, obtains `(state, code)`, and gets a signed-in admin's
    // browser to redeem it — attaching the attacker's Google account to the
    // admin's user. `PendingOauth` now records who started the ceremony.
    let server = google_mock("attacker-google-subject").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;

    // The member starts a link ceremony.
    let member_ctx = principal::member_context(member());
    let redirect = auth_context::with_context(member_ctx, f.svc.start_oauth(OauthProvider::Google))
        .await
        .unwrap();
    let state = state_from(&redirect.url);

    // The admin tries to complete it.
    let admin_ctx = principal::admin_context(admin());
    let result = auth_context::with_context(admin_ctx, f.svc.link_oauth(&state, "code")).await;

    assert!(
        matches!(result, Err(AppError::Forbidden(_))),
        "a ceremony started by another user must not be linkable: {result:?}"
    );
    assert!(
        f.credentials.rows.lock().unwrap().is_empty(),
        "no credential may be attached from somebody else's ceremony"
    );
}

#[tokio::test]
async fn a_sign_in_ceremony_cannot_be_redeemed_as_a_link() {
    // A ceremony started unauthenticated has no owner. Adopting it would let an
    // attacker who completed provider consent hand the resulting code to a
    // signed-in victim's browser.
    let server = google_mock("attacker-google-subject").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;

    // Started with no auth context at all — the sign-in entry point.
    let redirect = f.svc.start_oauth(OauthProvider::Google).await.unwrap();
    let state = state_from(&redirect.url);

    let ctx = principal::member_context(member());
    let result = auth_context::with_context(ctx, f.svc.link_oauth(&state, "code")).await;

    assert!(
        matches!(result, Err(AppError::Forbidden(_))),
        "an ownerless ceremony must not be adoptable: {result:?}"
    );
}

#[tokio::test]
async fn a_link_ceremony_started_by_the_caller_still_works() {
    // The guard must not break the legitimate flow.
    let server = google_mock("member-google-subject").await;
    let f = fixture(Some(&server.uri()));
    configure_google(&f).await;
    let ctx = principal::member_context(member());

    let redirect =
        auth_context::with_context(ctx.clone(), f.svc.start_oauth(OauthProvider::Google))
            .await
            .unwrap();
    let state = state_from(&redirect.url);

    auth_context::with_context(ctx, f.svc.link_oauth(&state, "code"))
        .await
        .expect("the caller who started the ceremony may complete it");

    assert_eq!(f.credentials.rows.lock().unwrap().len(), 1);
}

// -- provider token parsing -----------------------------------------------

/// An unrecognised path segment must not fall through to a configured
/// provider — that would let `/oauth/gooogle` start a real Google ceremony.
#[test]
fn provider_parsing_round_trips_and_rejects_the_unknown() {
    for provider in [OauthProvider::Google, OauthProvider::Github] {
        assert_eq!(
            OauthProvider::parse(provider.as_str()),
            Some(provider),
            "as_str and parse must agree"
        );
    }

    for raw in ["", "gooogle", "GOOGLE", "google ", "facebook", "wardnet"] {
        assert!(
            OauthProvider::parse(raw).is_none(),
            "{raw:?} must not parse as a provider"
        );
    }
}

/// The two providers must never share a credential kind, or linking one would
/// satisfy a lookup for the other.
#[test]
fn each_provider_has_its_own_credential_kind() {
    assert_ne!(
        OauthProvider::Google.credential_kind(),
        OauthProvider::Github.credential_kind()
    );
}

/// Config and secret keys must be provider-scoped, or configuring GitHub would
/// overwrite Google's client id.
#[test]
fn provider_keys_are_distinct_per_provider() {
    let (g, h) = (OauthProvider::Google, OauthProvider::Github);

    assert_ne!(g.client_id_key(), h.client_id_key());
    assert_ne!(g.enabled_key(), h.enabled_key());
    assert_ne!(g.secret_path(), h.secret_path());
}

// -- canonical hostname resolution ----------------------------------------

/// Cloudflare DDNS stores the hostname under a different key than the wardnet
/// provider does; both must resolve, or federated sign-in silently breaks for
/// one class of household.
#[tokio::test]
async fn the_canonical_hostname_resolves_for_cloudflare_households() {
    let config = Arc::new(MockSystemConfigRepo::empty());
    {
        let mut values = config.values.lock().unwrap();
        values.insert(KEY_PROVIDER.to_owned(), PROVIDER_CLOUDFLARE.to_owned());
        values.insert(KEY_DOMAIN.to_owned(), "home.example.com".to_owned());
    }
    let cfg = OauthConfig {
        system_config: config as Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
        secrets: Arc::new(MockSecretStore::empty())
            as Arc<dyn wardnetd_data::secret_store::SecretStore>,
    };

    assert_eq!(
        cfg.canonical_fqdn().await.unwrap().as_deref(),
        Some("home.example.com")
    );
}

/// No DDNS provider configured means no hostname a provider could redirect to,
/// so this resolves to `None` rather than guessing.
#[tokio::test]
async fn the_canonical_hostname_is_absent_without_a_ddns_provider() {
    for provider in [None, Some("some-future-provider")] {
        let config = Arc::new(MockSystemConfigRepo::empty());
        if let Some(p) = provider {
            config
                .values
                .lock()
                .unwrap()
                .insert(KEY_PROVIDER.to_owned(), p.to_owned());
        }
        let cfg = OauthConfig {
            system_config: config as Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
            secrets: Arc::new(MockSecretStore::empty())
                as Arc<dyn wardnetd_data::secret_store::SecretStore>,
        };

        assert!(
            cfg.canonical_fqdn().await.unwrap().is_none(),
            "provider {provider:?} must not yield a hostname"
        );
    }
}

/// A secret that is not valid UTF-8 is a corrupted vault, not a login failure:
/// it must surface as an internal error rather than panicking or being read as
/// an empty secret.
#[tokio::test]
async fn a_non_utf8_client_secret_is_an_internal_error() {
    let secrets = Arc::new(MockSecretStore::empty());
    wardnetd_data::secret_store::SecretStore::put(
        secrets.as_ref(),
        &OauthProvider::Google.secret_path(),
        &[0xff, 0xfe, 0xfd],
    )
    .await
    .unwrap();
    let cfg = OauthConfig {
        system_config: Arc::new(MockSystemConfigRepo::empty())
            as Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
        secrets: Arc::clone(&secrets) as Arc<dyn wardnetd_data::secret_store::SecretStore>,
    };

    let err = cfg
        .client_secret(OauthProvider::Google)
        .await
        .expect_err("a corrupted secret must not read as absent");
    assert!(
        matches!(err, AppError::Internal(_)),
        "expected an internal error, got {err:?}"
    );
}

/// An unconfigured provider has no secret — distinct from a corrupted one.
#[tokio::test]
async fn an_absent_client_secret_is_none() {
    let cfg = OauthConfig {
        system_config: Arc::new(MockSystemConfigRepo::empty())
            as Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
        secrets: Arc::new(MockSecretStore::empty())
            as Arc<dyn wardnetd_data::secret_store::SecretStore>,
    };

    assert!(
        cfg.client_secret(OauthProvider::Github)
            .await
            .unwrap()
            .is_none()
    );
}
