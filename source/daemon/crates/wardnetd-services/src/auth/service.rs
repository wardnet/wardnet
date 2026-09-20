use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{WizardMode, WizardStep};
use wardnet_common::auth::{AuthenticatedUser, UserRole};

use crate::auth::password::{
    hash_password, hash_token, new_session_token, validate_password, verify_decoy, verify_password,
};
use crate::auth::rate_limit::LoginRateLimiter;
use crate::auth_context;
use crate::error::AppError;
use wardnetd_data::repository::user::{UserRepository, UserRow};
use wardnetd_data::repository::user_credential::{CredentialKind, UserCredentialRepository};
use wardnetd_data::repository::{ApiKeyRepository, SessionRepository, SystemConfigRepository};

/// Snapshot of the setup-wizard progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WizardState {
    pub step: WizardStep,
    pub mode: Option<WizardMode>,
}

impl WizardState {
    /// Derived view used by the API and `SetupGuard`.
    #[must_use]
    pub fn setup_completed(&self) -> bool {
        self.step == WizardStep::Completed
    }
}

/// Successful login result returned to the API layer.
pub struct LoginResult {
    /// Raw session token to be set as a cookie.
    pub token: String,
    /// Cookie Max-Age in seconds.
    pub max_age_seconds: u64,
}

/// Everything a login attempt carries.
///
/// A struct rather than a widening parameter list because the two optional
/// fields are security-relevant and easy to pass in the wrong order as bare
/// `Option<&str>` arguments: `client_ip` feeds the rate limiter, `user_agent` is
/// shown in the "your sessions" list. Both are `None` for callers that genuinely
/// have neither (`wctl` over a unix socket, tests).
#[derive(Debug, Clone, Copy)]
pub struct LoginAttempt<'a> {
    /// Username or email address as typed. Matched case-insensitively.
    pub subject: &'a str,
    /// The submitted password.
    pub password: &'a str,
    /// Whether to issue a long-lived, refreshable session.
    pub remember_me: bool,
    /// Source address, for per-IP login backoff.
    pub client_ip: Option<&'a str>,
    /// Raw `User-Agent`, recorded on the session so a person can recognise it.
    pub user_agent: Option<&'a str>,
    /// Device the session is being issued to, when the caller is identifiable.
    pub device_id: Option<&'a str>,
}

/// The calling household user's identity, as returned by `GET /api/users/me`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentUser {
    /// The user's id.
    pub user_id: Uuid,
    /// Name shown in the UI. For a backfilled local admin this is the old
    /// `admins.username`.
    pub display_name: String,
    /// Optional email address.
    pub email: Option<String>,
    /// `admin` or `member`.
    pub role: UserRole,
}

/// Authentication and session management.
///
/// Orchestrates household-user login (password verification, session creation),
/// session validation (token → typed principal), and API-key validation. All
/// cryptographic operations live in [`crate::auth::password`]; repositories only
/// store and retrieve hashes.
#[async_trait]
pub trait AuthService: Send + Sync {
    /// Verify credentials and create a new session. Returns a raw token for the
    /// cookie.
    ///
    /// When `remember_me` is `true` the session is created with
    /// `remember_me_expiry_hours` lifetime instead of the default
    /// `session_expiry_hours`.
    ///
    /// Returns [`AppError::TooManyRequests`] when either login backoff counter
    /// has tripped, before any credential is verified.
    async fn login(&self, _attempt: LoginAttempt<'_>) -> Result<LoginResult, AppError>;

    /// Issue a session for a user whose credential has **already** been
    /// verified by another service.
    ///
    /// Exists for federated sign-in: `UserService::complete_oauth_callback`
    /// proves who somebody is, but session policy lives here, and duplicating
    /// it there would let the two drift (ADR-0031 §11).
    ///
    /// **This mints a credential from a bare user id.** Anyone who can call it
    /// can become any household user, which makes it the same class of
    /// primitive as [`AuthenticatedUser::from_validated_session`] — and it is
    /// policed by the same script,
    /// `build-support/check-auth-constructors.sh`. Before adding a call site,
    /// answer the question that script asks: *what credential did this code
    /// just verify?* If there isn't one, the answer is no.
    ///
    /// `remember_me` decides both the expiry and whether the session may later
    /// be slid forward by [`Self::refresh_session`].
    async fn issue_verified_session(
        &self,
        user_id: Uuid,
        remember_me: bool,
        user_agent: Option<&str>,
    ) -> Result<LoginResult, AppError>;

    /// Extend the expiry of an existing session (sliding-window refresh).
    ///
    /// Called by `POST /api/auth/refresh` on every app open. Validates that the
    /// session still exists and still belongs to a live, enabled user, slides
    /// the expiry forward, rotates the token, and returns the new token with its
    /// `max_age_seconds`.
    async fn refresh_session(&self, token: &str) -> Result<LoginResult, AppError>;

    /// Invalidate the session identified by the given raw token (logout).
    ///
    /// Backs `POST /api/auth/logout`. Callers must pass the token that
    /// authenticated the request (the API layer's `SessionAuth` guarantees
    /// this), so deleting it can only ever end the caller's own session.
    /// Idempotent: a token whose session row is already gone still succeeds,
    /// because the desired end state (no server-side session) already holds.
    async fn logout_session(&self, token: &str) -> Result<(), AppError>;

    /// Validate a raw session token into a typed principal.
    ///
    /// Returns [`AuthenticatedUser`] — carrying the user's **live** role read
    /// from `users` — rather than a bare id. A bare id forced the caller to
    /// decide what the session was allowed to do, and the caller
    /// (`resolve_auth_context`) answered "admin" for every session, which with
    /// two roles in play is a privilege escalation.
    async fn validate_session(&self, token: &str) -> Result<Option<AuthenticatedUser>, AppError>;

    /// Validate a raw API key into a typed principal.
    ///
    /// An API key is a box-level credential with no person attached, so it acts
    /// as the oldest enabled admin (see
    /// [`UserRepository::find_first_enabled_admin`]). If no enabled admin
    /// exists the key is refused rather than downgraded to some other user.
    async fn validate_api_key(&self, key: &str) -> Result<Option<AuthenticatedUser>, AppError>;

    /// Create the first admin account during initial setup.
    ///
    /// Validates the username (3–32 alphanumeric chars) and password, hashes the
    /// password with `Argon2id`, creates an `admin`-role household user with a
    /// `password` credential, and advances the wizard. Returns
    /// [`AppError::Conflict`] if setup has already been completed.
    async fn setup_admin(&self, username: &str, password: &str) -> Result<(), AppError>;

    /// Check whether the initial setup wizard has been completed.
    async fn is_setup_completed(&self) -> Result<bool, AppError>;

    /// Read the current setup-wizard state.
    ///
    /// Unauthenticated — `GET /api/setup/status` is exposed without a session
    /// so the web UI's `SetupGuard` can decide whether to redirect a fresh
    /// browser to the wizard. Documented exception to the
    /// `auth_context::require_admin()?` rule (see `.agents/auth.md`).
    async fn wizard_state(&self) -> Result<WizardState, AppError>;

    /// Return the calling household user's identity.
    ///
    /// Backs `GET /api/users/me`; identity comes from the request's
    /// [`AuthContext::User`](wardnet_common::auth::AuthContext) task-local.
    /// Guarded with `require_authenticated()`, not `require_admin()`: a member
    /// must be able to read their own profile.
    async fn current_user(&self) -> Result<CurrentUser, AppError>;

    /// Advance the wizard to the requested step.
    ///
    /// Validates that:
    /// - Forward transitions (of any distance) are always allowed.
    /// - Rewinds are allowed down to [`WizardStep::Network`] — never back to
    ///   [`WizardStep::Admin`] (admin creation is one-shot), and never out of
    ///   [`WizardStep::Completed`], which is terminal.
    /// - `mode` is either left unchanged or set when first reaching
    ///   [`WizardStep::Dhcp`].
    /// - Reaching [`WizardStep::Completed`] requires a user to exist.
    async fn advance_wizard(
        &self,
        to_step: WizardStep,
        mode: Option<WizardMode>,
    ) -> Result<WizardState, AppError>;

    /// Delete all sessions whose `expires_at` is in the past.
    ///
    /// Intended for the periodic
    /// [`SessionCleanupRunner`](crate::auth::SessionCleanupRunner); reads
    /// already filter expired rows, so this only reclaims dead storage.
    /// Returns the number of rows removed.
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError>;
}

/// Maximum lifetime of a `remember_me` session regardless of sliding-window
/// refreshes.
///
/// Written into `sessions.absolute_expires_at` at creation rather than
/// re-derived from `created_at` on every refresh, so the policy that issued a
/// session is the policy that bounds it.
const MAX_SESSION_DAYS: i64 = 90;

/// Default implementation of [`AuthService`] backed by repository traits.
pub struct AuthServiceImpl {
    users: Arc<dyn UserRepository>,
    credentials: Arc<dyn UserCredentialRepository>,
    sessions: Arc<dyn SessionRepository>,
    api_keys: Arc<dyn ApiKeyRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    rate_limiter: LoginRateLimiter,
    session_expiry_hours: u64,
    remember_me_expiry_hours: u64,
}

impl AuthServiceImpl {
    pub fn new(
        users: Arc<dyn UserRepository>,
        credentials: Arc<dyn UserCredentialRepository>,
        sessions: Arc<dyn SessionRepository>,
        api_keys: Arc<dyn ApiKeyRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        session_expiry_hours: u64,
        remember_me_expiry_hours: u64,
    ) -> Self {
        Self {
            users,
            credentials,
            sessions,
            api_keys,
            system_config,
            rate_limiter: LoginRateLimiter::new(),
            session_expiry_hours,
            remember_me_expiry_hours,
        }
    }

    /// Clamp an hour count to what `chrono::Duration::hours` accepts.
    fn expiry_hours_i64(hours: u64) -> i64 {
        hours.min(i64::MAX as u64).cast_signed()
    }

    /// The id of the household user the current context names.
    ///
    /// An exhaustive match, never a let-else: adding a principal must fail to
    /// compile at every point that decides *whose* data is being touched.
    fn context_user_id() -> Result<Uuid, AppError> {
        match auth_context::current() {
            wardnet_common::auth::AuthContext::User(user) => Ok(user.user_id()),
            wardnet_common::auth::AuthContext::Device { .. }
            | wardnet_common::auth::AuthContext::Anonymous => Err(AppError::Forbidden(
                "must be authenticated as a household user".to_owned(),
            )),
        }
    }
}

impl AuthServiceImpl {
    /// Create a session row and return its raw token.
    ///
    /// The **single** place session lifetime policy is applied — the
    /// normal-vs-`remember_me` expiry, the `MAX_SESSION_DAYS` absolute ceiling,
    /// and token hashing. Both the password login and the federated sign-in go
    /// through here rather than each computing their own bounds: a second copy
    /// would drift silently, and the half that drifted would be the half
    /// deciding how long somebody stays signed in.
    ///
    /// Callers must have just verified a credential. This is private precisely
    /// so that requirement is enforceable — the public door is
    /// [`AuthService::issue_verified_session`], which CI polices.
    async fn mint_session(
        &self,
        user_id: &str,
        remember_me: bool,
        device_id: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<LoginResult, AppError> {
        let (token, token_hash) = new_session_token();
        let session_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let expiry_hours = if remember_me {
            self.remember_me_expiry_hours
        } else {
            self.session_expiry_hours
        };
        let expires_at = now + chrono::Duration::hours(Self::expiry_hours_i64(expiry_hours));
        let absolute_expires_at = now + chrono::Duration::days(MAX_SESSION_DAYS);

        self.sessions
            .create(
                &session_id,
                user_id,
                &token_hash,
                &now.to_rfc3339(),
                &expires_at.to_rfc3339(),
                remember_me,
                device_id,
                user_agent,
                &absolute_expires_at.to_rfc3339(),
            )
            .await
            .map_err(AppError::Internal)?;

        Ok(LoginResult {
            token,
            max_age_seconds: expiry_hours * 3600,
        })
    }
}

#[async_trait]
impl AuthService for AuthServiceImpl {
    async fn login(&self, request: LoginAttempt<'_>) -> Result<LoginResult, AppError> {
        // Documented exception to the auth-guard rule (.agents/auth.md §Rules #2,
        // category (b): auth bootstrap): this IS the credential-verification
        // endpoint — by definition the caller has no session yet, so there is no
        // context to authenticate.

        // Backoff is checked before any lookup, so a throttled attempt costs an
        // attacker a round-trip and us nothing.
        if let Some(wait) = self
            .rate_limiter
            .check(request.subject, request.client_ip)
            .map(|d| d.as_secs().max(1))
        {
            tracing::warn!(retry_after = wait, "login throttled: retry_after={wait}s");
            return Err(AppError::TooManyRequests {
                message: format!("too many login attempts; retry in {wait}s"),
                retry_after_seconds: wait,
            });
        }

        // The subject is stored lowercased by both the migration backfill and
        // every write path, so the lookup lowercases too — logins are
        // case-insensitive in username and email alike.
        let subject = request.subject.trim().to_lowercase();

        let found = self
            .credentials
            .find_for_login(CredentialKind::Password, &subject)
            .await
            .map_err(AppError::Internal)?;

        // Unknown subject — or a disabled user, which `find_for_login`
        // deliberately makes indistinguishable — still runs a full Argon2id
        // verify against a decoy hash before rejecting. Short-circuiting here
        // would leak, via response latency, which accounts exist.
        let Some(login) = found else {
            verify_decoy(request.password);
            self.rate_limiter
                .record_failure(request.subject, request.client_ip);
            return Err(AppError::Unauthorized("invalid credentials".to_owned()));
        };

        let secret = login.credential.secret.as_deref().ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "password credential {} has no secret",
                login.credential.id
            ))
        })?;

        if let Err(e) = verify_password(request.password, secret) {
            self.rate_limiter
                .record_failure(request.subject, request.client_ip);
            return Err(e);
        }

        self.rate_limiter
            .record_success(request.subject, request.client_ip);

        let result = self
            .mint_session(
                &login.credential.user_id,
                request.remember_me,
                request.device_id,
                request.user_agent,
            )
            .await?;

        // Best-effort: a failure to stamp `last_used_at` must not fail a login
        // that has already succeeded.
        if let Err(e) = self
            .credentials
            .touch_last_used(&login.credential.id, &chrono::Utc::now().to_rfc3339())
            .await
        {
            tracing::warn!(error = %e, "failed to record credential last_used_at");
        }

        tracing::info!(
            user_id = %login.credential.user_id,
            role = login.role.as_str(),
            remember_me = request.remember_me,
            "login succeeded: role={}",
            login.role.as_str()
        );

        Ok(result)
    }

    async fn issue_verified_session(
        &self,
        user_id: Uuid,
        remember_me: bool,
        user_agent: Option<&str>,
    ) -> Result<LoginResult, AppError> {
        // Documented exception to the auth-guard rule (`.agents/auth.md`
        // category (b), auth bootstrap): this issues the very session that a
        // context would be read from, so requiring one is circular.
        //
        // The guard that matters for this method is not an `AuthContext` check
        // at all — it is *who is allowed to call it*. Minting a session from a
        // bare user id is the same class of primitive as
        // `AuthenticatedUser::from_validated_session`: whoever holds it can
        // become anybody. `build-support/check-auth-constructors.sh` polices the
        // call sites for exactly that reason, and the answer a new call site has
        // to give is the same one — *what credential did this code just verify?*
        self.mint_session(&user_id.to_string(), remember_me, None, user_agent)
            .await
    }

    async fn refresh_session(&self, token: &str) -> Result<LoginResult, AppError> {
        // Any authenticated household user may refresh their own session — a
        // member's session is no less refreshable than an admin's. Ownership,
        // not role, is the check that matters here, and it is enforced below.
        auth_context::require_authenticated()?;
        let ctx_user_id = Self::context_user_id()?;

        let token_hash = hash_token(token);
        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();

        // One atomic query: validates the session is live and returns
        // remember_me plus both expiry bounds, closing the race in which
        // `delete_expired` removes the row between two sequential reads.
        let session = self
            .sessions
            .find_session_for_refresh(&token_hash, &now_str)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("session not found or expired".to_owned()))?;

        // Cross-validate: the session row must belong to the calling user.
        let session_user_id = Uuid::parse_str(&session.user_id)
            .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid user_id in session row")))?;
        if session_user_id != ctx_user_id {
            return Err(AppError::Forbidden(
                "session does not belong to this user".to_owned(),
            ));
        }

        if !session.remember_me {
            return Err(AppError::Forbidden(
                "session was not created with remember_me - refresh not permitted".to_owned(),
            ));
        }

        // The absolute ceiling is read from the row, not recomputed: the policy
        // in force when the session was issued is the policy that bounds it.
        let absolute_expiry = chrono::DateTime::parse_from_rfc3339(&session.absolute_expires_at)
            .map_err(|_| {
                AppError::Internal(anyhow::anyhow!(
                    "invalid absolute_expires_at in session row"
                ))
            })?
            .with_timezone(&chrono::Utc);
        if now >= absolute_expiry {
            return Err(AppError::Unauthorized(
                "session has exceeded maximum lifetime - please log in again".to_owned(),
            ));
        }

        let slid_expiry =
            now + chrono::Duration::hours(Self::expiry_hours_i64(self.remember_me_expiry_hours));
        let new_expires_at = slid_expiry.min(absolute_expiry);

        // Rotate the token so a captured one cannot be re-used after refresh.
        let (new_token, new_token_hash) = new_session_token();

        self.sessions
            .rotate_token(&token_hash, &new_token_hash, &new_expires_at.to_rfc3339())
            .await
            .map_err(AppError::Internal)?;

        Ok(LoginResult {
            token: new_token,
            max_age_seconds: self.remember_me_expiry_hours * 3600,
        })
    }

    async fn logout_session(&self, token: &str) -> Result<(), AppError> {
        // Members log out too. The token is the one that authenticated this
        // request, so this can only ever end the caller's own session.
        auth_context::require_authenticated()?;

        let token_hash = hash_token(token);
        let removed = self
            .sessions
            .delete_by_token_hash(&token_hash)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(removed, "session logged out: removed={removed}");

        Ok(())
    }

    async fn validate_session(&self, token: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        // Documented exception to the auth-guard rule (.agents/auth.md §Rules #2,
        // category (b): auth bootstrap): this resolves a session token into an
        // identity, so it necessarily runs before any identity exists to
        // require.
        let token_hash = hash_token(token);
        let now = chrono::Utc::now().to_rfc3339();

        let principal = self
            .sessions
            .find_principal_by_token_hash(&token_hash, &now)
            .await
            .map_err(AppError::Internal)?;

        let Some(principal) = principal else {
            return Ok(None);
        };

        let user_id = Uuid::parse_str(&principal.user_id)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid UUID: {e}")))?;

        // The role travels from the `users` row this query joined, so a
        // demotion takes effect on the caller's next request rather than at
        // their next login.
        Ok(Some(AuthenticatedUser::from_validated_session(
            user_id,
            principal.role,
        )))
    }

    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        auth_context::require_admin()?;

        let now = chrono::Utc::now().to_rfc3339();
        self.sessions
            .delete_expired(&now)
            .await
            .map_err(AppError::Internal)
    }

    async fn validate_api_key(&self, key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        // Documented exception to the auth-guard rule (.agents/auth.md §Rules #2,
        // category (b): auth bootstrap): this resolves an API key into an
        // identity, so it necessarily runs before any identity exists to
        // require.
        let all_keys = self
            .api_keys
            .find_all_hashes()
            .await
            .map_err(AppError::Internal)?;

        for (id, key_hash) in &all_keys {
            let Ok(parsed_hash) = argon2::PasswordHash::new(key_hash) else {
                continue;
            };

            if argon2::PasswordVerifier::verify_password(
                &argon2::Argon2::default(),
                key.as_bytes(),
                &parsed_hash,
            )
            .is_ok()
            {
                let now = chrono::Utc::now().to_rfc3339();
                let _ = self.api_keys.update_last_used(id, &now).await;

                // An API key names no person, so it acts as the oldest enabled
                // admin. When there is none the key is refused: falling back to
                // any other user would hand a box-level credential a role
                // nobody granted it.
                let Some(admin) = self
                    .users
                    .find_first_enabled_admin()
                    .await
                    .map_err(AppError::Internal)?
                else {
                    tracing::warn!(
                        "api key accepted but no enabled admin exists; refusing the request"
                    );
                    return Ok(None);
                };

                let uuid = Uuid::parse_str(&admin.id)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid UUID: {e}")))?;

                return Ok(Some(AuthenticatedUser::from_validated_session(
                    uuid, admin.role,
                )));
            }
        }

        Ok(None)
    }

    async fn setup_admin(&self, username: &str, password: &str) -> Result<(), AppError> {
        // Documented exception to the auth-guard rule (`.agents/auth.md`):
        // by definition no user exists when this is called, so there is no
        // session to authenticate. The 409 guard below is the actual gate
        // — we read `users.exists()` directly rather than the legacy
        // `setup_completed` system_config key. That key was a separate write
        // from user creation, so a crash between the two could leave the
        // system claiming setup was incomplete when an account already
        // existed. Reading the row directly removes that race.

        // Guard: setup can only run once.
        if self.users.exists().await.map_err(AppError::Internal)? {
            return Err(AppError::Conflict("setup already completed".to_owned()));
        }

        // Validate username: non-empty, alphanumeric, 3-32 chars.
        if username.len() < 3
            || username.len() > 32
            || !username.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(AppError::BadRequest(
                "username must be 3-32 alphanumeric characters".to_owned(),
            ));
        }

        validate_password(password)?;

        let password_hash = hash_password(password)?;
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        self.users
            .create(&UserRow {
                id: id.clone(),
                display_name: username.to_owned(),
                // No email: the wizard only asks for a username, and inventing
                // `username@localhost` would occupy the unique email index with
                // an address nobody can receive mail at.
                email: None,
                role: UserRole::Admin,
                enabled: true,
                created_at: now.clone(),
                updated_at: now.clone(),
            })
            .await
            .map_err(AppError::Internal)?;

        // Same compensation as `bootstrap_admin`: a user row with no credential
        // makes `exists()` true forever, which skips this guard, blocks the
        // wizard, and leaves nobody able to log in. Roll it back rather than
        // leaving the box permanently half-claimed.
        if let Err(e) = self
            .credentials
            .set_password(
                &Uuid::new_v4().to_string(),
                &id,
                // Lowercased so the break-glass login is case-insensitive,
                // matching how the migration backfills existing local admins.
                &username.to_lowercase(),
                &password_hash,
                &now,
            )
            .await
        {
            if let Err(cleanup) = self.users.delete(&id).await {
                tracing::error!(
                    error = %cleanup,
                    user_id = %id,
                    "failed to roll back the half-created admin; the box may be \
                     left with a credential-less user that blocks the wizard"
                );
            }
            return Err(AppError::Internal(e));
        }

        // Advance the wizard in a single write. `is_setup_completed()` is
        // derived from `wizard_step == Completed`, so the legacy
        // `setup_completed` key is no longer maintained here. If this write
        // fails after user creation the 409 guard above still fires on retry,
        // and the operator can recover via POST /api/setup/advance.
        //
        // Only advance from "admin" or unset; if wizard_step is already further
        // along we leave it alone — same-step advances are idempotent in
        // `advance_wizard`, and rewinding below `Network` is rejected there.
        let current = self
            .system_config
            .get_wizard_step()
            .await
            .map_err(AppError::Internal)?;
        if current.as_deref() == Some(WizardStep::Admin.as_storage_str()) || current.is_none() {
            self.system_config
                .set_wizard_step(WizardStep::Network.as_storage_str())
                .await
                .map_err(AppError::Internal)?;
        }

        tracing::info!(username = %username, "setup completed: admin account created for username={username}");

        Ok(())
    }

    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        // Documented exception to the auth-guard rule (.agents/auth.md §Rules #2,
        // category (b): auth bootstrap): backs the unauthenticated
        // `GET /api/setup/status` surface and delegates to the equally-unguarded
        // `wizard_state`, so there is no session to require here.
        Ok(self.wizard_state().await?.setup_completed())
    }

    async fn current_user(&self) -> Result<CurrentUser, AppError> {
        // Deliberately `require_authenticated`, not `require_admin`: a member
        // reading their own profile is the whole point of this endpoint.
        auth_context::require_authenticated()?;
        let user_id = Self::context_user_id()?;

        let row = self
            .users
            .find_by_id(&user_id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("user account no longer exists".to_owned()))?;

        Ok(CurrentUser {
            user_id,
            display_name: row.display_name,
            email: row.email,
            role: row.role,
        })
    }

    async fn wizard_state(&self) -> Result<WizardState, AppError> {
        // Documented exception: this is exposed via the unauthenticated
        // `GET /api/setup/status` endpoint, so it deliberately does not
        // call `auth_context::require_*()`.
        let step = self
            .system_config
            .get_wizard_step()
            .await
            .map_err(AppError::Internal)?
            .map_or(WizardStep::Admin, |s| WizardStep::from_storage_str(&s));

        let mode = self
            .system_config
            .get_wizard_mode()
            .await
            .map_err(AppError::Internal)?
            .and_then(|s| WizardMode::from_storage_str(&s));

        Ok(WizardState { step, mode })
    }

    async fn advance_wizard(
        &self,
        to_step: WizardStep,
        mode: Option<WizardMode>,
    ) -> Result<WizardState, AppError> {
        auth_context::require_admin()?;

        let current = self.wizard_state().await?;

        if current.step == WizardStep::Completed && to_step != WizardStep::Completed {
            return Err(AppError::BadRequest(
                "wizard is completed; cannot rewind".to_owned(),
            ));
        }

        if to_step.ordinal() < current.step.ordinal() && to_step == WizardStep::Admin {
            return Err(AppError::BadRequest(format!(
                "wizard cannot rewind from {} to admin",
                current.step.as_storage_str(),
            )));
        }

        if to_step == WizardStep::Completed {
            // Sanity-check: can't finish setup without an account.
            let user_exists = self.users.exists().await.map_err(AppError::Internal)?;
            if !user_exists {
                return Err(AppError::BadRequest(
                    "cannot complete wizard before an admin is created".to_owned(),
                ));
            }
        }

        // Only update mode when explicitly provided, so callers don't need to
        // re-send it on every step.
        let new_mode = mode.or(current.mode);

        self.system_config
            .set_wizard_step(to_step.as_storage_str())
            .await
            .map_err(AppError::Internal)?;
        if let Some(m) = new_mode {
            self.system_config
                .set_wizard_mode(m.as_storage_str())
                .await
                .map_err(AppError::Internal)?;
        }

        tracing::info!(
            from = current.step.as_storage_str(),
            to = to_step.as_storage_str(),
            mode = ?new_mode,
            "wizard advanced"
        );

        Ok(WizardState {
            step: to_step,
            mode: new_mode,
        })
    }
}
