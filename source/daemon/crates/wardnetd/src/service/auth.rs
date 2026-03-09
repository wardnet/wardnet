use std::sync::Arc;

use argon2::PasswordHasher;
use argon2::password_hash::rand_core::OsRng;
use async_trait::async_trait;
use base64::Engine;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::AppError;
use crate::repository::{
    AdminRepository, ApiKeyRepository, SessionRepository, SystemConfigRepository,
};

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
    async fn login(&self, username: &str, password: &str) -> Result<LoginResult, AppError>;

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
}

/// Default implementation of [`AuthService`] backed by repository traits.
pub struct AuthServiceImpl {
    admins: Arc<dyn AdminRepository>,
    sessions: Arc<dyn SessionRepository>,
    api_keys: Arc<dyn ApiKeyRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    session_expiry_hours: u64,
}

impl AuthServiceImpl {
    pub fn new(
        admins: Arc<dyn AdminRepository>,
        sessions: Arc<dyn SessionRepository>,
        api_keys: Arc<dyn ApiKeyRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        session_expiry_hours: u64,
    ) -> Self {
        Self {
            admins,
            sessions,
            api_keys,
            system_config,
            session_expiry_hours,
        }
    }
}

#[async_trait]
impl AuthService for AuthServiceImpl {
    async fn login(&self, username: &str, password: &str) -> Result<LoginResult, AppError> {
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
        let expiry_hours = i64::try_from(self.session_expiry_hours).unwrap_or(24);
        let expires_at = now + chrono::Duration::hours(expiry_hours);

        self.sessions
            .create(
                &session_id,
                &admin_id,
                &token_hash,
                &now.to_rfc3339(),
                &expires_at.to_rfc3339(),
            )
            .await
            .map_err(AppError::Internal)?;

        Ok(LoginResult {
            token,
            max_age_seconds: self.session_expiry_hours * 3600,
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
        // Guard: setup can only run once.
        let completed = self
            .system_config
            .is_setup_completed()
            .await
            .map_err(AppError::Internal)?;
        if completed {
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

        self.system_config
            .set_setup_completed(true)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(username = %username, "setup completed: admin account created for username={username}");

        Ok(())
    }

    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        self.system_config
            .is_setup_completed()
            .await
            .map_err(AppError::Internal)
    }
}
