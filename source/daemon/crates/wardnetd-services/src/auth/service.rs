use std::sync::Arc;

use argon2::PasswordHasher;
use argon2::password_hash::rand_core::OsRng;
use async_trait::async_trait;
use base64::Engine;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wardnet_common::api::{WizardMode, WizardStep};

use crate::auth_context;
use crate::error::AppError;
use wardnetd_data::repository::{
    AdminRepository, ApiKeyRepository, SessionRepository, SystemConfigRepository,
};

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

/// Authentication and session management.
///
/// Orchestrates admin login (password verification, session creation),
/// session validation (token → admin lookup), and API-key validation.
/// All cryptographic operations (argon2, SHA-256) live here — repositories
/// only store and retrieve hashes.
#[async_trait]
pub trait AuthService: Send + Sync {
    /// Verify credentials and create a new session. Returns a raw token for the cookie.
    ///
    /// When `remember_me` is `true`, the session is created with
    /// `remember_me_expiry_hours` lifetime instead of the default
    /// `session_expiry_hours`.
    async fn login(
        &self,
        username: &str,
        password: &str,
        remember_me: bool,
    ) -> Result<LoginResult, AppError>;

    /// Extend the expiry of an existing session (sliding-window refresh).
    ///
    /// Called by `POST /api/auth/refresh` on every admin-app open. Validates
    /// that the session still exists, slides the expiry forward by
    /// `remember_me_expiry_hours`, and returns the same token with the new
    /// `max_age_seconds`.
    async fn refresh_session(&self, token: &str) -> Result<LoginResult, AppError>;

    /// Validate a raw session token. Returns the admin UUID if valid and not expired.
    async fn validate_session(&self, token: &str) -> Result<Option<Uuid>, AppError>;

    /// Validate a raw API key. Returns the admin UUID if a matching key is found.
    async fn validate_api_key(&self, key: &str) -> Result<Option<Uuid>, AppError>;

    /// Create the first admin account during initial setup.
    ///
    /// Validates the username (3-32 alphanumeric chars) and password (min 8 chars),
    /// hashes the password with argon2, creates the admin, and marks setup as completed.
    /// Returns [`AppError::Conflict`] if setup has already been completed.
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

    /// Advance the wizard to the requested step.
    ///
    /// Validates that:
    /// - Step transitions only move forward (no rewinds).
    /// - `mode` is either left unchanged or set when first reaching
    ///   [`WizardStep::Dhcp`].
    /// - Reaching [`WizardStep::Completed`] requires an admin to exist.
    async fn advance_wizard(
        &self,
        to_step: WizardStep,
        mode: Option<WizardMode>,
    ) -> Result<WizardState, AppError>;
}

/// Maximum lifetime of a `remember_me` session regardless of sliding-window refreshes.
const MAX_SESSION_DAYS: i64 = 90;

/// Default implementation of [`AuthService`] backed by repository traits.
pub struct AuthServiceImpl {
    admins: Arc<dyn AdminRepository>,
    sessions: Arc<dyn SessionRepository>,
    api_keys: Arc<dyn ApiKeyRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    session_expiry_hours: u64,
    remember_me_expiry_hours: u64,
}

impl AuthServiceImpl {
    pub fn new(
        admins: Arc<dyn AdminRepository>,
        sessions: Arc<dyn SessionRepository>,
        api_keys: Arc<dyn ApiKeyRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        session_expiry_hours: u64,
        remember_me_expiry_hours: u64,
    ) -> Self {
        Self {
            admins,
            sessions,
            api_keys,
            system_config,
            session_expiry_hours,
            remember_me_expiry_hours,
        }
    }
}

#[async_trait]
impl AuthService for AuthServiceImpl {
    async fn login(
        &self,
        username: &str,
        password: &str,
        remember_me: bool,
    ) -> Result<LoginResult, AppError> {
        // Documented exception to the auth-guard rule (.agents/auth.md §Rules #2):
        // this IS the credential-verification endpoint — by definition the caller
        // has no session yet, so there is no context to authenticate.
        let (admin_id, password_hash) = self
            .admins
            .find_by_username(username)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("invalid credentials".to_owned()))?;

        let parsed_hash = argon2::PasswordHash::new(&password_hash)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid stored hash: {e}")))?;

        argon2::PasswordVerifier::verify_password(
            &argon2::Argon2::default(),
            password.as_bytes(),
            &parsed_hash,
        )
        .map_err(|_| AppError::Unauthorized("invalid credentials".to_owned()))?;

        // Generate random 32-byte token, base64url-encode, SHA-256 hash for storage.
        let token_bytes: [u8; 32] = rand::random();
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
        let token_hash = hex::encode(Sha256::digest(token.as_bytes()));

        let session_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now();
        let expiry_hours = if remember_me {
            self.remember_me_expiry_hours
        } else {
            self.session_expiry_hours
        };
        let expiry_hours_i64 = expiry_hours.min(i64::MAX as u64).cast_signed();
        let expires_at = now + chrono::Duration::hours(expiry_hours_i64);

        self.sessions
            .create(
                &session_id,
                &admin_id,
                &token_hash,
                &now.to_rfc3339(),
                &expires_at.to_rfc3339(),
                remember_me,
            )
            .await
            .map_err(AppError::Internal)?;

        Ok(LoginResult {
            token,
            max_age_seconds: expiry_hours * 3600,
        })
    }

    async fn refresh_session(&self, token: &str) -> Result<LoginResult, AppError> {
        auth_context::require_admin()?;

        // Extract the calling admin's identity for cross-validation below.
        let wardnet_common::auth::AuthContext::Admin {
            admin_id: ctx_admin_id,
        } = auth_context::current()
        else {
            return Err(AppError::Forbidden(
                "must be authenticated as admin".to_owned(),
            ));
        };

        let token_hash = hex::encode(Sha256::digest(token.as_bytes()));
        let now = chrono::Utc::now();
        let now_str = now.to_rfc3339();

        // Single atomic query: validates the session is non-expired and retrieves
        // remember_me + created_at in one round-trip, eliminating the race window where
        // delete_expired() could remove the row between two sequential reads.
        let (session_admin_id, is_refreshable, created_at_str) = self
            .sessions
            .find_session_for_refresh(&token_hash, &now_str)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("session not found or expired".to_owned()))?;

        // Cross-validate: the session row must belong to the calling admin.
        let session_admin_uuid = Uuid::parse_str(&session_admin_id)
            .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid admin_id in session row")))?;
        if session_admin_uuid != ctx_admin_id {
            return Err(AppError::Forbidden(
                "session does not belong to this admin".to_owned(),
            ));
        }

        if !is_refreshable {
            return Err(AppError::Forbidden(
                "session was not created with remember_me — refresh not permitted".to_owned(),
            ));
        }

        // Enforce an absolute lifetime cap so remember_me sessions cannot refresh indefinitely.
        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map_err(|_| AppError::Internal(anyhow::anyhow!("invalid created_at in session row")))?
            .with_timezone(&chrono::Utc);
        let absolute_expiry = created_at + chrono::Duration::days(MAX_SESSION_DAYS);
        if now >= absolute_expiry {
            return Err(AppError::Unauthorized(
                "session has exceeded maximum lifetime — please log in again".to_owned(),
            ));
        }

        let expiry_hours_i64 = self
            .remember_me_expiry_hours
            .min(i64::MAX as u64)
            .cast_signed();
        let slid_expiry = now + chrono::Duration::hours(expiry_hours_i64);
        let new_expires_at = slid_expiry.min(absolute_expiry);

        // Rotate token: generate a fresh secret so a captured token cannot be re-used.
        let new_token_bytes: [u8; 32] = rand::random();
        let new_token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(new_token_bytes);
        let new_token_hash = hex::encode(Sha256::digest(new_token.as_bytes()));

        self.sessions
            .rotate_token(&token_hash, &new_token_hash, &new_expires_at.to_rfc3339())
            .await
            .map_err(AppError::Internal)?;

        Ok(LoginResult {
            token: new_token,
            max_age_seconds: self.remember_me_expiry_hours * 3600,
        })
    }

    async fn validate_session(&self, token: &str) -> Result<Option<Uuid>, AppError> {
        let token_hash = hex::encode(Sha256::digest(token.as_bytes()));
        let now = chrono::Utc::now().to_rfc3339();

        let admin_id_str = self
            .sessions
            .find_admin_id_by_token_hash(&token_hash, &now)
            .await
            .map_err(AppError::Internal)?;

        match admin_id_str {
            Some(id) => {
                let uuid = Uuid::parse_str(&id)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid UUID: {e}")))?;
                Ok(Some(uuid))
            }
            None => Ok(None),
        }
    }

    async fn validate_api_key(&self, key: &str) -> Result<Option<Uuid>, AppError> {
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

                // In the single-admin MVP, API keys authenticate as the first admin.
                let admin_id_str = self
                    .admins
                    .find_first_id()
                    .await
                    .map_err(AppError::Internal)?
                    .ok_or_else(|| {
                        AppError::Internal(anyhow::anyhow!("no admin account exists"))
                    })?;

                let uuid = Uuid::parse_str(&admin_id_str)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid UUID: {e}")))?;

                return Ok(Some(uuid));
            }
        }

        Ok(None)
    }

    async fn setup_admin(&self, username: &str, password: &str) -> Result<(), AppError> {
        // Documented exception to the auth-guard rule (`.agents/auth.md`):
        // by definition no admin exists when this is called, so there's no
        // session to authenticate. The 409 guard below is the actual gate
        // — we use `admins.exists()` directly rather than the legacy
        // `setup_completed` system_config key. That key was the previous
        // signal but it was a separate write from `admin_repo.create()`,
        // so a crash between the two could leave the system in a state
        // where an admin exists but the key is `false` (or vice versa) —
        // the 409 check would then disagree with reality. Reading the
        // admin row directly removes that race entirely.

        // Guard: setup can only run once.
        if self.admins.exists().await.map_err(AppError::Internal)? {
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

        // Validate password: minimum 8 chars.
        if password.len() < 8 {
            return Err(AppError::BadRequest(
                "password must be at least 8 characters".to_owned(),
            ));
        }

        // Hash password with argon2.
        let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
        let password_hash = argon2::Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to hash password: {e}")))?
            .to_string();

        let id = Uuid::new_v4().to_string();
        self.admins
            .create(&id, username, &password_hash)
            .await
            .map_err(AppError::Internal)?;

        // Advance the wizard to the next step in a single write. The
        // `setup_completed` key is no longer maintained by this method —
        // `is_setup_completed()` is now derived from
        // `wizard_step == Completed` (see below). If this write fails
        // after admin creation, the 409 guard above still fires on
        // retry (the admin row exists), and the operator can recover
        // by hitting POST /api/setup/advance from the wizard UI.
        //
        // Only advance from "admin" or unset state; if wizard_step is
        // already further along (e.g. an operator hit advance manually
        // before the frontend got to it) we leave it alone — same-step
        // advances are idempotent in `advance_wizard`, but rewinding
        // explicitly past `Network` would just hit advance_wizard's
        // ordinal check.
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
        // Derived from `wizard_step == Completed` so this matches the
        // value the API surfaces in `SetupStatusResponse.setup_completed`.
        // The legacy `setup_completed` key in `system_config` is no
        // longer written by `setup_admin` (it would race against the
        // wizard_step write); it's kept only as a migration signal that
        // `bootstrap_system_config` reads on first boot of an upgraded
        // install.
        Ok(self.wizard_state().await?.setup_completed())
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

        if to_step.ordinal() < current.step.ordinal() {
            return Err(AppError::BadRequest(format!(
                "wizard cannot rewind from {} to {}",
                current.step.as_storage_str(),
                to_step.as_storage_str()
            )));
        }

        if to_step == WizardStep::Completed {
            // Sanity-check: can't finish setup without an admin.
            let admin_exists = self.admins.exists().await.map_err(AppError::Internal)?;
            if !admin_exists {
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
