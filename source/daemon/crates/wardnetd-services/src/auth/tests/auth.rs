//! Tests for [`AuthServiceImpl`] — login, session validation, refresh, logout
//! and API-key validation, on the household-identity model (ADR-0031).

use std::sync::Arc;

use uuid::Uuid;
use wardnet_common::auth::UserRole;
use wardnet_test_support::principal;
use wardnetd_data::repository::session::SessionRepository;
use wardnetd_data::repository::system_config::SystemConfigRepository;
use wardnetd_data::repository::user::UserRepository;
use wardnetd_data::repository::user_credential::UserCredentialRepository;

use crate::auth::password::hash_token;
use crate::auth::{AuthService, AuthServiceImpl, LoginAttempt};
use crate::auth_context;
use crate::error::AppError;
use crate::tests::repo_mocks::{
    MockApiKeyRepo, MockCredentialRepo, MockSessionRepo, MockSystemConfigRepo, MockUserRepo,
    StoredSession, user_row,
};

/// The backfilled local admin's id on every box in these tests.
const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
/// An ordinary household member.
const MEMBER_ID: &str = "00000000-0000-0000-0000-000000000002";

const SESSION_HOURS: u64 = 24;
const REMEMBER_HOURS: u64 = 720;

/// Argon2id-hash a password with a fixed salt, so tests are deterministic.
fn argon2_hash(password: &str) -> String {
    use argon2::PasswordHasher;
    let salt = argon2::password_hash::SaltString::from_b64("dGVzdHNhbHR2YWx1ZTEyMw").unwrap();
    argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

/// A `LoginAttempt` with no client metadata — the shape most tests want.
fn attempt<'a>(subject: &'a str, password: &'a str, remember_me: bool) -> LoginAttempt<'a> {
    LoginAttempt {
        subject,
        password,
        remember_me,
        client_ip: None,
        user_agent: None,
        device_id: None,
    }
}

/// The wired-up service plus the handles a test needs to assert against.
struct Fixture {
    svc: AuthServiceImpl,
    users: Arc<MockUserRepo>,
    credentials: Arc<MockCredentialRepo>,
    sessions: Arc<MockSessionRepo>,
    config: Arc<MockSystemConfigRepo>,
}

/// Build a service over the given repositories, joining sessions and
/// credentials to `users` so the `enabled` filters behave as they do in SQL.
fn fixture(
    users: MockUserRepo,
    credentials: MockCredentialRepo,
    api_keys: MockApiKeyRepo,
) -> Fixture {
    let users = Arc::new(users);
    let credentials = Arc::new(credentials);
    let sessions = Arc::new(MockSessionRepo::joined_to(Arc::clone(&users)));
    let config = Arc::new(MockSystemConfigRepo::empty());
    let svc = AuthServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::clone(&credentials) as Arc<dyn UserCredentialRepository>,
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        Arc::new(api_keys),
        Arc::clone(&config) as Arc<dyn SystemConfigRepository>,
        SESSION_HOURS,
        REMEMBER_HOURS,
    );
    Fixture {
        svc,
        users,
        credentials,
        sessions,
        config,
    }
}

/// A box with one enabled admin whose password is `password`.
fn admin_with_password(password: &str) -> Fixture {
    let users = MockUserRepo::with_admin(ADMIN_ID, "admin");
    let users_arc = Arc::new(users);
    let credentials = MockCredentialRepo::joined_to(Arc::clone(&users_arc)).with_password(
        "cred-1",
        ADMIN_ID,
        "admin",
        &argon2_hash(password),
    );
    let sessions = Arc::new(MockSessionRepo::joined_to(Arc::clone(&users_arc)));
    let credentials = Arc::new(credentials);
    let config = Arc::new(MockSystemConfigRepo::empty());
    let svc = AuthServiceImpl::new(
        Arc::clone(&users_arc) as Arc<dyn UserRepository>,
        Arc::clone(&credentials) as Arc<dyn UserCredentialRepository>,
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        Arc::new(MockApiKeyRepo::empty()),
        Arc::clone(&config) as Arc<dyn SystemConfigRepository>,
        SESSION_HOURS,
        REMEMBER_HOURS,
    );
    Fixture {
        svc,
        users: users_arc,
        credentials,
        sessions,
        config,
    }
}

/// A live session row for `user_id`, valid for a day and refreshable for 90.
fn live_session(user_id: &str, token: &str, remember_me: bool) -> StoredSession {
    let now = chrono::Utc::now();
    StoredSession {
        id: "session-1".to_owned(),
        user_id: user_id.to_owned(),
        token_hash: hash_token(token),
        created_at: now.to_rfc3339(),
        expires_at: (now + chrono::Duration::days(1)).to_rfc3339(),
        remember_me,
        device_id: None,
        user_agent: None,
        absolute_expires_at: (now + chrono::Duration::days(90)).to_rfc3339(),
    }
}

// -- login ----------------------------------------------------------------

#[tokio::test]
async fn login_success() {
    let f = admin_with_password("correct-password");

    let login = f
        .svc
        .login(attempt("admin", "correct-password", false))
        .await
        .expect("login should succeed");

    assert!(!login.token.is_empty());
    assert_eq!(login.max_age_seconds, SESSION_HOURS * 3600);

    // The session row is written against the credential's user, with the
    // absolute ceiling stored rather than left to be re-derived on refresh.
    let row = f.sessions.only();
    assert_eq!(row.user_id, ADMIN_ID);
    assert_eq!(row.token_hash, hash_token(&login.token));
    assert!(!row.remember_me);
    assert!(row.absolute_expires_at > row.expires_at);

    // A successful login stamps the credential.
    assert!(f.credentials.last_used("cred-1").is_some());
}

#[tokio::test]
async fn login_success_with_remember_me() {
    let f = admin_with_password("correct-password");

    let login = f
        .svc
        .login(attempt("admin", "correct-password", true))
        .await
        .unwrap();

    assert_eq!(login.max_age_seconds, REMEMBER_HOURS * 3600);
    assert!(f.sessions.only().remember_me);
}

#[tokio::test]
async fn login_is_case_insensitive_in_the_subject() {
    // The migration lowercases backfilled usernames and every write path
    // lowercases too, so `Admin` and `admin` are the same account. If the
    // lookup stopped folding case, every upgraded box would lose its login.
    let f = admin_with_password("correct-password");

    let login = f
        .svc
        .login(attempt("ADMIN", "correct-password", false))
        .await;
    assert!(login.is_ok(), "mixed-case username must still authenticate");
}

#[tokio::test]
async fn login_with_malformed_stored_hash_returns_internal() {
    // A corrupt credential must surface as Internal, not masquerade as bad
    // credentials — otherwise the operator hunts a forgotten password instead
    // of a broken row.
    let users = Arc::new(MockUserRepo::with_admin(ADMIN_ID, "admin"));
    let credentials = MockCredentialRepo::joined_to(Arc::clone(&users)).with_password(
        "cred-1",
        ADMIN_ID,
        "admin",
        "not-an-argon2-hash",
    );
    let f = fixture(
        MockUserRepo::with_admin(ADMIN_ID, "admin"),
        credentials,
        MockApiKeyRepo::empty(),
    );

    let result = f.svc.login(attempt("admin", "any-password", false)).await;
    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn login_wrong_password() {
    let f = admin_with_password("correct-password");
    let result = f.svc.login(attempt("admin", "wrong-password", false)).await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
    // Nothing was issued.
    assert_eq!(f.sessions.count(), 0);
}

#[tokio::test]
async fn login_user_not_found() {
    let f = fixture(
        MockUserRepo::empty(),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::empty(),
    );
    let result = f.svc.login(attempt("nobody", "password", false)).await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
}

#[tokio::test]
async fn login_refuses_a_disabled_user() {
    // Disabling is the revocation primitive: it must stop authentication
    // immediately, without anybody having to hunt down live sessions. The
    // credential is perfectly valid here — only `users.enabled` differs.
    let users = Arc::new(MockUserRepo::with_rows(vec![user_row(
        ADMIN_ID,
        "admin",
        UserRole::Admin,
        false,
    )]));
    let credentials = Arc::new(
        MockCredentialRepo::joined_to(Arc::clone(&users)).with_password(
            "cred-1",
            ADMIN_ID,
            "admin",
            &argon2_hash("correct-password"),
        ),
    );
    let sessions = Arc::new(MockSessionRepo::joined_to(Arc::clone(&users)));
    let svc = AuthServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::clone(&credentials) as Arc<dyn UserCredentialRepository>,
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        Arc::new(MockApiKeyRepo::empty()),
        Arc::new(MockSystemConfigRepo::empty()),
        SESSION_HOURS,
        REMEMBER_HOURS,
    );

    let result = svc.login(attempt("admin", "correct-password", false)).await;
    // Indistinguishable from an unknown subject, so a disabled account cannot
    // be probed by watching which error comes back.
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
    assert_eq!(sessions.count(), 0);
}

#[tokio::test]
async fn login_unknown_subject_runs_decoy_verify_to_hide_timing() {
    // Enumeration mitigation: an unknown subject must still trigger a full
    // Argon2id verify against a decoy hash so its latency matches a known
    // subject with the wrong password. Short-circuiting would complete in
    // microseconds while the wrong-password path spends milliseconds — a side
    // channel an attacker could use to enumerate accounts.
    let known = admin_with_password("correct-password");
    let unknown = fixture(
        MockUserRepo::empty(),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::empty(),
    );

    // Warm up so neither measurement pays one-time init (allocator pages, the
    // lazily-computed decoy hash).
    let _ = known.svc.login(attempt("admin", "warm", false)).await;
    let _ = unknown.svc.login(attempt("nobody", "warm", false)).await;

    let wrong_password = {
        let start = std::time::Instant::now();
        let r = known
            .svc
            .login(attempt("admin", "wrong-password", false))
            .await;
        assert!(matches!(r, Err(AppError::Unauthorized(_))));
        start.elapsed()
    };
    let missing_user = {
        let start = std::time::Instant::now();
        let r = unknown
            .svc
            .login(attempt("nobody", "wrong-password", false))
            .await;
        assert!(matches!(r, Err(AppError::Unauthorized(_))));
        start.elapsed()
    };

    // Both do exactly one Argon2id verify, so the durations sit on the same
    // order of magnitude. The 4x slack keeps this robust under CI scheduling
    // noise while still catching a regression that drops the decoy verify.
    assert!(
        missing_user * 4 >= wrong_password,
        "unknown-subject login ({missing_user:?}) was far faster than a wrong-password \
         login ({wrong_password:?}); the decoy argon2 verify looks to be missing"
    );
}

#[tokio::test]
async fn repeated_failures_trip_the_login_rate_limiter() {
    // Per-identity backoff (ADR-0031 §12). The exact free-attempt count is a
    // tuning decision, so this asserts the property that matters: sustained
    // wrong guesses eventually stop being answered with a plain 401, and the
    // refusal carries a retry hint.
    let f = admin_with_password("correct-password");

    let mut throttled = None;
    for _ in 0..12 {
        match f.svc.login(attempt("admin", "wrong-password", false)).await {
            Err(AppError::TooManyRequests {
                retry_after_seconds,
                ..
            }) => {
                throttled = Some(retry_after_seconds);
                break;
            }
            Err(AppError::Unauthorized(_)) => {}
            Ok(_) => panic!("a wrong password must never succeed"),
            Err(other) => panic!("unexpected login error: {other}"),
        }
    }

    let retry_after = throttled.expect("sustained wrong passwords must eventually be throttled");
    assert!(
        retry_after >= 1,
        "a throttled login must tell the caller how long to wait"
    );
}

#[tokio::test]
async fn a_successful_login_clears_the_backoff() {
    // Otherwise one person's typos would lock out the household member who
    // then types their password correctly.
    let f = admin_with_password("correct-password");

    for _ in 0..3 {
        let _ = f.svc.login(attempt("admin", "wrong-password", false)).await;
    }
    assert!(
        f.svc
            .login(attempt("admin", "correct-password", false))
            .await
            .is_ok()
    );
    // And the next attempt is not sitting behind the earlier failures.
    assert!(
        f.svc
            .login(attempt("admin", "correct-password", false))
            .await
            .is_ok()
    );
}

// -- validate_session -----------------------------------------------------

#[tokio::test]
async fn validate_session_returns_the_typed_principal() {
    let f = admin_with_password("pw");
    f.sessions
        .rows
        .lock()
        .unwrap()
        .push(live_session(ADMIN_ID, "raw-token", false));

    let user = f
        .svc
        .validate_session("raw-token")
        .await
        .unwrap()
        .expect("a live session must resolve");

    assert_eq!(user.user_id().to_string(), ADMIN_ID);
    assert_eq!(user.role(), UserRole::Admin);
}

#[tokio::test]
async fn validate_session_carries_the_members_own_role_not_admin() {
    // The escalation this epic exists to close: `resolve_auth_context` used to
    // promote *any* valid session to admin. The role must come from the
    // `users` row the session joins to.
    let users = Arc::new(MockUserRepo::with_rows(vec![
        user_row(ADMIN_ID, "admin", UserRole::Admin, true),
        user_row(MEMBER_ID, "kid", UserRole::Member, true),
    ]));
    let sessions = Arc::new(
        MockSessionRepo::joined_to(Arc::clone(&users)).with_session(live_session(
            MEMBER_ID,
            "member-token",
            false,
        )),
    );
    let svc = AuthServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::new(MockCredentialRepo::empty()),
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        Arc::new(MockApiKeyRepo::empty()),
        Arc::new(MockSystemConfigRepo::empty()),
        SESSION_HOURS,
        REMEMBER_HOURS,
    );

    let user = svc
        .validate_session("member-token")
        .await
        .unwrap()
        .expect("the member's session is live");

    assert_eq!(
        user.role(),
        UserRole::Member,
        "a member must not become admin"
    );
    assert!(
        !wardnet_common::auth::AuthContext::user(user).is_admin(),
        "a member context must fail require_admin"
    );
}

#[tokio::test]
async fn validate_session_rejects_a_disabled_users_live_session() {
    // Disabling must revoke *now*, not at the next login.
    let users = Arc::new(MockUserRepo::with_rows(vec![user_row(
        ADMIN_ID,
        "admin",
        UserRole::Admin,
        false,
    )]));
    let sessions = Arc::new(
        MockSessionRepo::joined_to(Arc::clone(&users)).with_session(live_session(
            ADMIN_ID,
            "raw-token",
            true,
        )),
    );
    let svc = AuthServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::new(MockCredentialRepo::empty()),
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        Arc::new(MockApiKeyRepo::empty()),
        Arc::new(MockSystemConfigRepo::empty()),
        SESSION_HOURS,
        REMEMBER_HOURS,
    );

    assert!(svc.validate_session("raw-token").await.unwrap().is_none());
}

#[tokio::test]
async fn validate_session_with_malformed_user_id_returns_internal() {
    // A session row whose user_id is not a UUID must error rather than
    // silently authenticate or deny.
    let f = admin_with_password("pw");
    // The user row has to exist, or the liveness join would filter the session
    // out before the service ever tries to parse the id — and this test is
    // about the parse, not the join.
    f.users
        .rows
        .lock()
        .unwrap()
        .push(user_row("not-a-uuid", "corrupt", UserRole::Admin, true));
    let mut row = live_session(ADMIN_ID, "raw-token", false);
    row.user_id = "not-a-uuid".to_owned();
    f.sessions.rows.lock().unwrap().push(row);

    let result = f.svc.validate_session("raw-token").await;
    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn validate_session_expired() {
    let f = admin_with_password("pw");
    let now = chrono::Utc::now();
    let mut row = live_session(ADMIN_ID, "raw-token", false);
    row.expires_at = (now - chrono::Duration::hours(1)).to_rfc3339();
    f.sessions.rows.lock().unwrap().push(row);

    assert!(f.svc.validate_session("raw-token").await.unwrap().is_none());
}

#[tokio::test]
async fn validate_session_past_its_absolute_ceiling_is_refused() {
    // The ceiling is what stops a remember_me session refreshing forever.
    let f = admin_with_password("pw");
    let now = chrono::Utc::now();
    let mut row = live_session(ADMIN_ID, "raw-token", true);
    row.absolute_expires_at = (now - chrono::Duration::minutes(1)).to_rfc3339();
    f.sessions.rows.lock().unwrap().push(row);

    assert!(f.svc.validate_session("raw-token").await.unwrap().is_none());
}

#[tokio::test]
async fn validate_session_unknown_token() {
    let f = admin_with_password("pw");
    assert!(f.svc.validate_session("nope").await.unwrap().is_none());
}

// -- validate_api_key -----------------------------------------------------

#[tokio::test]
async fn validate_api_key_acts_as_the_oldest_enabled_admin() {
    let f = fixture(
        MockUserRepo::with_admin(ADMIN_ID, "admin"),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::with_hashes(vec![("key-1".to_owned(), argon2_hash("my-secret-key"))]),
    );

    let user = f
        .svc
        .validate_api_key("my-secret-key")
        .await
        .unwrap()
        .expect("a matching key authenticates");

    assert_eq!(user.user_id().to_string(), ADMIN_ID);
    assert_eq!(user.role(), UserRole::Admin);
}

#[tokio::test]
async fn validate_api_key_invalid() {
    let f = fixture(
        MockUserRepo::with_admin(ADMIN_ID, "admin"),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::with_hashes(vec![("key-1".to_owned(), argon2_hash("my-secret-key"))]),
    );

    assert!(f.svc.validate_api_key("wrong-key").await.unwrap().is_none());
}

#[tokio::test]
async fn validate_api_key_no_keys_returns_none() {
    let f = fixture(
        MockUserRepo::with_admin(ADMIN_ID, "admin"),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::empty(),
    );
    assert!(f.svc.validate_api_key("any-key").await.unwrap().is_none());
}

#[tokio::test]
async fn validate_api_key_skips_malformed_hash() {
    // One malformed hash must not stop the valid one from matching.
    let f = fixture(
        MockUserRepo::with_admin(ADMIN_ID, "admin"),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::with_hashes(vec![
            ("key-bad".to_owned(), "not-a-valid-argon2-hash".to_owned()),
            ("key-good".to_owned(), argon2_hash("valid-key")),
        ]),
    );

    assert!(f.svc.validate_api_key("valid-key").await.unwrap().is_some());
}

#[tokio::test]
async fn validate_api_key_is_refused_when_no_enabled_admin_exists() {
    // A key borrows an admin's authority. With every admin disabled there is
    // no authority to borrow, and falling back to some other user would hand
    // the key a role nobody granted it.
    let f = fixture(
        MockUserRepo::with_rows(vec![
            user_row(ADMIN_ID, "admin", UserRole::Admin, false),
            user_row(MEMBER_ID, "kid", UserRole::Member, true),
        ]),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::with_hashes(vec![("key-1".to_owned(), argon2_hash("my-secret-key"))]),
    );

    assert!(
        f.svc
            .validate_api_key("my-secret-key")
            .await
            .unwrap()
            .is_none(),
        "with no enabled admin the key must be refused, not downgraded to the member"
    );
}

// -- current_user ---------------------------------------------------------

#[tokio::test]
async fn current_user_requires_authentication() {
    let f = admin_with_password("pw");
    let result = f.svc.current_user().await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn current_user_returns_the_callers_profile() {
    let f = admin_with_password("pw");
    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());

    let user = auth_context::with_context(ctx, async { f.svc.current_user().await })
        .await
        .unwrap();

    assert_eq!(user.user_id.to_string(), ADMIN_ID);
    assert_eq!(user.display_name, "admin");
    assert_eq!(user.role, UserRole::Admin);
}

#[tokio::test]
async fn current_user_is_available_to_a_member() {
    // `/api/users/me` is guarded with require_authenticated, not require_admin:
    // a member reading their own profile is the point of the endpoint.
    let f = fixture(
        MockUserRepo::with_rows(vec![user_row(MEMBER_ID, "kid", UserRole::Member, true)]),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::empty(),
    );
    let ctx = principal::member_context(Uuid::parse_str(MEMBER_ID).unwrap());

    let user = auth_context::with_context(ctx, async { f.svc.current_user().await })
        .await
        .unwrap();

    assert_eq!(user.display_name, "kid");
    assert_eq!(user.role, UserRole::Member);
}

#[tokio::test]
async fn current_user_rejects_a_device_caller() {
    // A device holds no credential and has no profile to read.
    let f = admin_with_password("pw");
    let ctx = principal::device_context("AA:BB:CC:DD:EE:01");

    let result = auth_context::with_context(ctx, async { f.svc.current_user().await }).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn current_user_errors_when_the_account_is_gone() {
    // A live session whose user row was deleted must not resolve to a profile.
    let f = fixture(
        MockUserRepo::empty(),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::empty(),
    );
    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());

    let result = auth_context::with_context(ctx, async { f.svc.current_user().await }).await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
}

// -- setup / wizard delegation -------------------------------------------

#[tokio::test]
async fn is_setup_completed_is_false_on_a_fresh_box() {
    let f = fixture(
        MockUserRepo::empty(),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::empty(),
    );
    assert!(!f.svc.is_setup_completed().await.unwrap());
}

// -- cleanup --------------------------------------------------------------

#[tokio::test]
async fn cleanup_expired_sessions_requires_admin() {
    let f = admin_with_password("pw");
    let result = f.svc.cleanup_expired_sessions().await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn cleanup_expired_sessions_removes_only_expired_rows() {
    let f = admin_with_password("pw");
    let now = chrono::Utc::now();
    let mut expired = live_session(ADMIN_ID, "old-token", false);
    expired.expires_at = (now - chrono::Duration::hours(1)).to_rfc3339();
    f.sessions.rows.lock().unwrap().push(expired);
    f.sessions
        .rows
        .lock()
        .unwrap()
        .push(live_session(ADMIN_ID, "live-token", false));

    let removed = auth_context::with_context(wardnet_common::auth::AuthContext::system(), async {
        f.svc.cleanup_expired_sessions().await
    })
    .await
    .unwrap();

    assert_eq!(removed, 1);
    assert_eq!(f.sessions.count(), 1);
}

// -- refresh --------------------------------------------------------------

#[tokio::test]
async fn refresh_session_rotates_the_token() {
    let f = admin_with_password("pw");
    f.sessions
        .rows
        .lock()
        .unwrap()
        .push(live_session(ADMIN_ID, "old-token", true));
    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());

    let result =
        auth_context::with_context(ctx, async { f.svc.refresh_session("old-token").await })
            .await
            .unwrap();

    assert!(!result.token.is_empty());
    assert_ne!(result.token, "old-token");
    assert_eq!(result.max_age_seconds, REMEMBER_HOURS * 3600);
    // The stored hash is the new token's: a captured old token is now useless.
    assert_eq!(f.sessions.only().token_hash, hash_token(&result.token));
}

#[tokio::test]
async fn refresh_session_not_remember_me_returns_forbidden() {
    let f = admin_with_password("pw");
    f.sessions
        .rows
        .lock()
        .unwrap()
        .push(live_session(ADMIN_ID, "old-token", false));
    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());

    let result =
        auth_context::with_context(ctx, async { f.svc.refresh_session("old-token").await }).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn refresh_session_refuses_another_users_session() {
    // Ownership, not role, is the check that matters: the session row must
    // belong to the calling principal.
    let users = Arc::new(MockUserRepo::with_rows(vec![
        user_row(ADMIN_ID, "admin", UserRole::Admin, true),
        user_row(MEMBER_ID, "kid", UserRole::Member, true),
    ]));
    let sessions = Arc::new(
        MockSessionRepo::joined_to(Arc::clone(&users)).with_session(live_session(
            MEMBER_ID,
            "kids-token",
            true,
        )),
    );
    let svc = AuthServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::new(MockCredentialRepo::empty()),
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        Arc::new(MockApiKeyRepo::empty()),
        Arc::new(MockSystemConfigRepo::empty()),
        SESSION_HOURS,
        REMEMBER_HOURS,
    );

    // The admin presents the member's token.
    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());
    let result =
        auth_context::with_context(ctx, async { svc.refresh_session("kids-token").await }).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn refresh_session_refuses_a_disabled_user() {
    // The refresh lookup applies the same four liveness conditions as
    // validation. If it did not, disabling an account would leave a
    // refreshable session standing — a revocation that does not revoke.
    let users = Arc::new(MockUserRepo::with_rows(vec![user_row(
        ADMIN_ID,
        "admin",
        UserRole::Admin,
        false,
    )]));
    let sessions = Arc::new(
        MockSessionRepo::joined_to(Arc::clone(&users)).with_session(live_session(
            ADMIN_ID,
            "old-token",
            true,
        )),
    );
    let svc = AuthServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::new(MockCredentialRepo::empty()),
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        Arc::new(MockApiKeyRepo::empty()),
        Arc::new(MockSystemConfigRepo::empty()),
        SESSION_HOURS,
        REMEMBER_HOURS,
    );

    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());
    let result =
        auth_context::with_context(ctx, async { svc.refresh_session("old-token").await }).await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
}

#[tokio::test]
async fn refresh_session_past_the_absolute_ceiling_returns_unauthorized() {
    let f = admin_with_password("pw");
    let mut row = live_session(ADMIN_ID, "old-token", true);
    // Inside the sliding window, but past the hard ceiling.
    row.absolute_expires_at = (chrono::Utc::now() + chrono::Duration::seconds(1)).to_rfc3339();
    f.sessions.rows.lock().unwrap().push(row);
    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());

    // Wait past the ceiling without waiting 90 days.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let result =
        auth_context::with_context(ctx, async { f.svc.refresh_session("old-token").await }).await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
}

#[tokio::test]
async fn refresh_session_with_malformed_absolute_expiry_returns_internal() {
    // The ceiling cannot be enforced without a parseable timestamp, so this
    // must error rather than refresh.
    let f = admin_with_password("pw");
    let mut row = live_session(ADMIN_ID, "old-token", true);
    row.absolute_expires_at = "not-a-timestamp".to_owned();
    f.sessions.rows.lock().unwrap().push(row);
    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());

    let result =
        auth_context::with_context(ctx, async { f.svc.refresh_session("old-token").await }).await;
    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn refresh_session_unknown_token_returns_unauthorized() {
    let f = admin_with_password("pw");
    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());

    let result =
        auth_context::with_context(ctx, async { f.svc.refresh_session("nope").await }).await;
    assert!(matches!(result, Err(AppError::Unauthorized(_))));
}

#[tokio::test]
async fn refresh_session_requires_authentication() {
    let f = admin_with_password("pw");
    let result = f.svc.refresh_session("any-token").await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// -- logout ---------------------------------------------------------------

#[tokio::test]
async fn logout_session_requires_authentication() {
    let f = admin_with_password("pw");
    let result = f.svc.logout_session("any-token").await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn logout_session_deletes_the_row_by_token_hash() {
    let f = admin_with_password("pw");
    f.sessions
        .rows
        .lock()
        .unwrap()
        .push(live_session(ADMIN_ID, "raw-token", false));
    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());

    auth_context::with_context(ctx, async { f.svc.logout_session("raw-token").await })
        .await
        .unwrap();

    assert_eq!(f.sessions.count(), 0);
    // And the session no longer resolves.
    assert!(f.svc.validate_session("raw-token").await.unwrap().is_none());
}

#[tokio::test]
async fn logout_session_is_available_to_a_member() {
    // Members log out too — this is guarded with require_authenticated.
    let users = Arc::new(MockUserRepo::with_rows(vec![user_row(
        MEMBER_ID,
        "kid",
        UserRole::Member,
        true,
    )]));
    let sessions = Arc::new(
        MockSessionRepo::joined_to(Arc::clone(&users)).with_session(live_session(
            MEMBER_ID,
            "kids-token",
            false,
        )),
    );
    let svc = AuthServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::new(MockCredentialRepo::empty()),
        Arc::clone(&sessions) as Arc<dyn SessionRepository>,
        Arc::new(MockApiKeyRepo::empty()),
        Arc::new(MockSystemConfigRepo::empty()),
        SESSION_HOURS,
        REMEMBER_HOURS,
    );

    let ctx = principal::member_context(Uuid::parse_str(MEMBER_ID).unwrap());
    let result =
        auth_context::with_context(ctx, async { svc.logout_session("kids-token").await }).await;
    assert!(result.is_ok());
    assert_eq!(sessions.count(), 0);
}

#[tokio::test]
async fn logout_session_is_idempotent_when_the_row_is_already_gone() {
    // The desired end state (no server-side session) already holds.
    let f = admin_with_password("pw");
    let ctx = principal::admin_context(Uuid::parse_str(ADMIN_ID).unwrap());

    let result =
        auth_context::with_context(ctx, async { f.svc.logout_session("any-token").await }).await;
    assert!(result.is_ok());
}

// -- setup_admin ----------------------------------------------------------

#[tokio::test]
async fn setup_admin_creates_a_user_and_a_password_credential() {
    let f = fixture(
        MockUserRepo::empty(),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::empty(),
    );

    f.svc
        .setup_admin("operator", "a-good-password")
        .await
        .unwrap();

    assert_eq!(f.users.count(), 1);
    let rows = f.users.rows.lock().unwrap().clone();
    assert_eq!(rows[0].display_name, "operator");
    assert_eq!(rows[0].role, UserRole::Admin);
    assert!(rows[0].enabled);
    // The email index is left free rather than filled with a fake address.
    assert!(rows[0].email.is_none());

    // The credential subject is lowercased, so login is case-insensitive.
    let credential = f
        .credentials
        .find_password(&rows[0].id)
        .await
        .unwrap()
        .expect("a password credential must exist");
    assert_eq!(credential.subject, "operator");

    // And the wizard moved off the admin step.
    assert_eq!(f.config.read("wizard_step").as_deref(), Some("network"));
}

#[tokio::test]
async fn setup_admin_refuses_once_a_user_exists() {
    let f = fixture(
        MockUserRepo::with_admin(ADMIN_ID, "admin"),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::empty(),
    );

    let result = f.svc.setup_admin("second", "a-good-password").await;
    assert!(matches!(result, Err(AppError::Conflict(_))));
}

#[tokio::test]
async fn setup_admin_validates_the_username_and_password() {
    let f = fixture(
        MockUserRepo::empty(),
        MockCredentialRepo::empty(),
        MockApiKeyRepo::empty(),
    );

    assert!(matches!(
        f.svc.setup_admin("ab", "a-good-password").await,
        Err(AppError::BadRequest(_))
    ));
    assert!(matches!(
        f.svc.setup_admin("has space", "a-good-password").await,
        Err(AppError::BadRequest(_))
    ));
    assert!(matches!(
        f.svc.setup_admin("operator", "short").await,
        Err(AppError::BadRequest(_))
    ));
    // Nothing was written by any rejected attempt.
    assert_eq!(f.users.count(), 0);
}

#[tokio::test]
async fn setup_admin_rolls_back_the_user_when_the_credential_write_fails() {
    // Without compensating, `users.exists()` stays true forever: the 409 guard
    // fires on every retry, the wizard considers itself done, and nobody can
    // log in. That is a permanent lockout with no recovery short of editing the
    // database by hand.
    let users = Arc::new(MockUserRepo::empty());
    let credentials = Arc::new(
        MockCredentialRepo::joined_to(Arc::clone(&users)).failing_set_password("disk on fire"),
    );
    let svc = AuthServiceImpl::new(
        Arc::clone(&users) as Arc<dyn UserRepository>,
        Arc::clone(&credentials) as Arc<dyn UserCredentialRepository>,
        Arc::new(MockSessionRepo::empty()),
        Arc::new(MockApiKeyRepo::empty()),
        Arc::new(MockSystemConfigRepo::empty()),
        SESSION_HOURS,
        REMEMBER_HOURS,
    );

    let result = svc.setup_admin("operator", "a-good-password").await;
    assert!(matches!(result, Err(AppError::Internal(_))));
    assert_eq!(
        users.count(),
        0,
        "the orphan users row must be rolled back, or the box is permanently half-claimed"
    );
}
