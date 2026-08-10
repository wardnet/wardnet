//! The household user directory and enrolment (ADR-0031).

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::auth::UserRole;

use crate::auth::password::{hash_password, hash_token, new_session_token, validate_password};
use crate::auth_context;
use crate::error::AppError;
use wardnetd_data::repository::session::SessionRepository;
use wardnetd_data::repository::user::{DuplicateUserEmailError, UserRepository, UserRow};
use wardnetd_data::repository::user_credential::{CredentialSummary, UserCredentialRepository};
use wardnetd_data::repository::user_enrolment::{EnrolmentTokenRow, UserEnrolmentRepository};

/// How long an admin-issued enrolment token stays redeemable.
///
/// Long enough to hand a phone to a family member over a weekend, short enough
/// that a forgotten invitation is not a standing way in.
const ENROLMENT_TTL_HOURS: i64 = 72;

/// A household user as the admin directory shows them.
///
/// No credential material of any kind — not even a "has password" flag derived
/// from a secret. Credentials are listed separately, and always as
/// [`CredentialSummary`], which structurally has no secret field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProfile {
    pub id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub role: UserRole,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl UserProfile {
    /// Convert a stored row, failing on an unparseable id.
    fn from_row(row: UserRow) -> Result<Self, AppError> {
        Ok(Self {
            id: Uuid::parse_str(&row.id).map_err(|e| {
                AppError::Internal(anyhow::anyhow!("user {} has an invalid id: {e}", row.id))
            })?,
            display_name: row.display_name,
            email: row.email,
            role: row.role,
            enabled: row.enabled,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// What an admin supplies to create a household user.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub display_name: String,
    pub email: Option<String>,
    pub role: UserRole,
}

/// A freshly issued enrolment invitation.
///
/// The `token` is present **exactly once**, in this value. Only its hash is
/// persisted, so if the admin loses it the only recourse is to issue another —
/// which is the intended property, not a limitation.
#[derive(Debug, Clone)]
pub struct EnrolmentInvite {
    /// The token to hand over, out of band.
    pub token: String,
    /// RFC 3339 expiry, so the UI can say how long it is good for.
    pub expires_at: String,
    /// The user being enrolled.
    pub user_id: Uuid,
}

/// An outstanding or spent invitation, for the admin UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrolmentSummary {
    pub id: Uuid,
    pub user_id: Uuid,
    pub created_at: String,
    pub expires_at: String,
    pub used_at: Option<String>,
}

/// The household user directory.
///
/// Every method opens with an `auth_context::require_*()` guard, with the one
/// documented exception noted on [`redeem_enrolment`](UserService::redeem_enrolment)
/// — see `.agents/auth.md`.
#[async_trait]
pub trait UserService: Send + Sync {
    /// Every household user, for the admin directory.
    async fn list_users(&self) -> Result<Vec<UserProfile>, AppError>;

    /// One user by id.
    ///
    /// Readable by an admin, or by that user about themselves. A member must
    /// not be able to enumerate the household by walking ids.
    async fn get_user(&self, user_id: Uuid) -> Result<UserProfile, AppError>;

    /// Create a household user with **no credential**.
    ///
    /// The account cannot be used until it is enrolled: the admin never learns
    /// the member's password (ADR-0031 §3), so account creation and credential
    /// creation are deliberately separate steps.
    async fn create_user(&self, new_user: NewUser) -> Result<UserProfile, AppError>;

    /// Update a user's display name and email.
    ///
    /// A user may edit their own profile; changing anybody else's is an admin
    /// action.
    async fn update_profile(
        &self,
        user_id: Uuid,
        display_name: &str,
        email: Option<&str>,
    ) -> Result<UserProfile, AppError>;

    /// Enable or disable a user.
    ///
    /// Disabling revokes immediately: every live session is deleted, and the
    /// login join filters the account out from the next request onward.
    /// Refuses to disable the last enabled admin.
    async fn set_enabled(&self, user_id: Uuid, enabled: bool) -> Result<UserProfile, AppError>;

    /// Change a user's role. Refuses to demote the last enabled admin.
    async fn set_role(&self, user_id: Uuid, role: UserRole) -> Result<UserProfile, AppError>;

    /// Delete a user. Refuses to delete the last enabled admin.
    ///
    /// Credentials, enrolment tokens and sessions cascade;
    /// `devices.owner_user_id` is set to NULL — deleting a person must not
    /// delete the household's hardware.
    async fn delete_user(&self, user_id: Uuid) -> Result<(), AppError>;

    /// List a user's credentials, without secrets.
    async fn list_credentials(&self, user_id: Uuid) -> Result<Vec<CredentialSummary>, AppError>;

    /// Set a user's own password, verifying the current one first.
    ///
    /// Every other live session for that user is invalidated: a credential
    /// change that leaves sessions standing has not revoked anything.
    async fn change_own_password(
        &self,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), AppError>;

    /// Issue a one-time enrolment token for a user who has no password yet.
    async fn issue_enrolment(&self, user_id: Uuid) -> Result<EnrolmentInvite, AppError>;

    /// Outstanding and spent invitations for a user.
    async fn list_enrolments(&self, user_id: Uuid) -> Result<Vec<EnrolmentSummary>, AppError>;

    /// Revoke an unredeemed invitation.
    async fn revoke_enrolment(&self, enrolment_id: Uuid) -> Result<(), AppError>;

    /// Redeem an enrolment token, setting the member's first password.
    ///
    /// **Documented exception** to the auth-guard rule (`.agents/auth.md`
    /// category (b), auth bootstrap): the caller is a household member who has
    /// no credential yet and therefore cannot have a session. The token *is*
    /// the authorization, which is why it is single-use, expiring, and checked
    /// in SQL rather than here.
    async fn redeem_enrolment(&self, token: &str, password: &str) -> Result<UserProfile, AppError>;

    /// Delete expired enrolment tokens. Called by the session-cleanup runner.
    async fn cleanup_expired_enrolments(&self) -> Result<u64, AppError>;
}

/// Default [`UserService`] over the repository traits.
pub struct UserServiceImpl {
    users: Arc<dyn UserRepository>,
    credentials: Arc<dyn UserCredentialRepository>,
    enrolments: Arc<dyn UserEnrolmentRepository>,
    sessions: Arc<dyn SessionRepository>,
}

impl UserServiceImpl {
    pub fn new(
        users: Arc<dyn UserRepository>,
        credentials: Arc<dyn UserCredentialRepository>,
        enrolments: Arc<dyn UserEnrolmentRepository>,
        sessions: Arc<dyn SessionRepository>,
    ) -> Self {
        Self {
            users,
            credentials,
            enrolments,
            sessions,
        }
    }

    /// The calling user's id, or `Forbidden` for a device/anonymous caller.
    ///
    /// An exhaustive match, never a let-else: adding a principal must fail to
    /// compile at every point that decides *whose* data is being touched.
    fn caller() -> Result<Uuid, AppError> {
        match auth_context::current() {
            wardnet_common::auth::AuthContext::User(user) => Ok(user.user_id()),
            wardnet_common::auth::AuthContext::Device { .. }
            | wardnet_common::auth::AuthContext::Anonymous => Err(AppError::Forbidden(
                "must be authenticated as a household user".to_owned(),
            )),
        }
    }

    /// Allow the action if the caller is an admin **or** is acting on
    /// themselves.
    ///
    /// The self case is checked before the admin case so a member editing their
    /// own profile never depends on role at all.
    fn require_admin_or_self(user_id: Uuid) -> Result<(), AppError> {
        auth_context::require_authenticated()?;
        if Self::caller()? == user_id {
            return Ok(());
        }
        auth_context::require_admin()
    }

    /// Map a repository error, translating the email-uniqueness marker into a
    /// `409` rather than letting it surface as a `500`.
    fn map_write_error(err: anyhow::Error) -> AppError {
        if err.downcast_ref::<DuplicateUserEmailError>().is_some() {
            return AppError::Conflict("a household user with that email already exists".to_owned());
        }
        AppError::Internal(err)
    }

    /// Validate a display name.
    fn validate_display_name(display_name: &str) -> Result<(), AppError> {
        let trimmed = display_name.trim();
        if trimmed.is_empty() || trimmed.chars().count() > 64 {
            return Err(AppError::BadRequest(
                "display name must be 1-64 characters".to_owned(),
            ));
        }
        Ok(())
    }

    /// Validate an email address, if one was given.
    ///
    /// Deliberately shallow: one `@` with something either side. Anything
    /// stricter rejects addresses that genuinely work, and this field is a login
    /// identifier and a label, not something the daemon sends mail to.
    fn validate_email(email: Option<&str>) -> Result<Option<String>, AppError> {
        let Some(email) = email else { return Ok(None) };
        let trimmed = email.trim();
        if trimmed.is_empty() {
            // An empty string would occupy the unique index with a value that
            // means "none"; store NULL instead.
            return Ok(None);
        }
        let mut parts = trimmed.splitn(2, '@');
        let local = parts.next().unwrap_or_default();
        let domain = parts.next().unwrap_or_default();
        if local.is_empty() || domain.is_empty() || !domain.contains('.') {
            return Err(AppError::BadRequest(
                "email address is not valid".to_owned(),
            ));
        }
        Ok(Some(trimmed.to_lowercase()))
    }

    /// Read a user or return `404`.
    async fn load(&self, user_id: Uuid) -> Result<UserRow, AppError> {
        self.users
            .find_by_id(&user_id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("household user not found".to_owned()))
    }

    /// Refuse an action that would leave the household with no way in.
    ///
    /// Applies when the target is currently an enabled admin and the action
    /// removes that: demotion, disabling, or deletion. A box with no enabled
    /// admin cannot be administered at all, and the local password is the
    /// break-glass path — there is no recovery short of editing the database.
    async fn guard_last_admin(&self, target: &UserRow, action: &str) -> Result<(), AppError> {
        if target.role != UserRole::Admin || !target.enabled {
            return Ok(());
        }
        let enabled_admins = self
            .users
            .count_enabled_admins()
            .await
            .map_err(AppError::Internal)?;
        if enabled_admins <= 1 {
            return Err(AppError::Conflict(format!(
                "cannot {action} the last enabled admin; promote another user first"
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl UserService for UserServiceImpl {
    async fn list_users(&self) -> Result<Vec<UserProfile>, AppError> {
        auth_context::require_admin()?;

        self.users
            .find_all()
            .await
            .map_err(AppError::Internal)?
            .into_iter()
            .map(UserProfile::from_row)
            .collect()
    }

    async fn get_user(&self, user_id: Uuid) -> Result<UserProfile, AppError> {
        Self::require_admin_or_self(user_id)?;
        UserProfile::from_row(self.load(user_id).await?)
    }

    async fn create_user(&self, new_user: NewUser) -> Result<UserProfile, AppError> {
        auth_context::require_admin()?;
        Self::validate_display_name(&new_user.display_name)?;
        let email = Self::validate_email(new_user.email.as_deref())?;

        let now = chrono::Utc::now().to_rfc3339();
        let row = UserRow {
            id: Uuid::new_v4().to_string(),
            display_name: new_user.display_name.trim().to_owned(),
            email,
            role: new_user.role,
            enabled: true,
            created_at: now.clone(),
            updated_at: now,
        };

        self.users
            .create(&row)
            .await
            .map_err(Self::map_write_error)?;

        tracing::info!(
            user_id = %row.id,
            role = row.role.as_str(),
            "household user created: role={}",
            row.role.as_str()
        );

        UserProfile::from_row(row)
    }

    async fn update_profile(
        &self,
        user_id: Uuid,
        display_name: &str,
        email: Option<&str>,
    ) -> Result<UserProfile, AppError> {
        Self::require_admin_or_self(user_id)?;
        Self::validate_display_name(display_name)?;
        let email = Self::validate_email(email)?;

        let now = chrono::Utc::now().to_rfc3339();
        let affected = self
            .users
            .update_profile(
                &user_id.to_string(),
                display_name.trim(),
                email.as_deref(),
                &now,
            )
            .await
            .map_err(Self::map_write_error)?;
        if affected == 0 {
            return Err(AppError::NotFound("household user not found".to_owned()));
        }

        UserProfile::from_row(self.load(user_id).await?)
    }

    async fn set_enabled(&self, user_id: Uuid, enabled: bool) -> Result<UserProfile, AppError> {
        auth_context::require_admin()?;

        let target = self.load(user_id).await?;
        if !enabled {
            self.guard_last_admin(&target, "disable").await?;
        }

        let now = chrono::Utc::now().to_rfc3339();
        self.users
            .set_enabled(&user_id.to_string(), enabled, &now)
            .await
            .map_err(AppError::Internal)?;

        if !enabled {
            // Disabling must revoke *now*. The login and validation joins would
            // reject the account anyway, so this is belt-and-braces — but it is
            // also what makes "sign out everywhere" observably true, and it
            // reclaims the rows immediately rather than at the next cleanup.
            let removed = self
                .sessions
                .delete_all_for_user(&user_id.to_string())
                .await
                .map_err(AppError::Internal)?;
            tracing::info!(
                user_id = %user_id,
                removed,
                "user disabled, sessions revoked: removed={removed}"
            );
        }

        UserProfile::from_row(self.load(user_id).await?)
    }

    async fn set_role(&self, user_id: Uuid, role: UserRole) -> Result<UserProfile, AppError> {
        auth_context::require_admin()?;

        let target = self.load(user_id).await?;
        if role != UserRole::Admin {
            self.guard_last_admin(&target, "demote").await?;
        }

        let now = chrono::Utc::now().to_rfc3339();
        self.users
            .set_role(&user_id.to_string(), role, &now)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            user_id = %user_id,
            role = role.as_str(),
            "household user role changed: role={}",
            role.as_str()
        );

        UserProfile::from_row(self.load(user_id).await?)
    }

    async fn delete_user(&self, user_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let target = self.load(user_id).await?;
        self.guard_last_admin(&target, "delete").await?;

        self.users
            .delete(&user_id.to_string())
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(user_id = %user_id, "household user deleted");
        Ok(())
    }

    async fn list_credentials(&self, user_id: Uuid) -> Result<Vec<CredentialSummary>, AppError> {
        Self::require_admin_or_self(user_id)?;

        self.credentials
            .list_for_user(&user_id.to_string())
            .await
            .map_err(AppError::Internal)
    }

    async fn change_own_password(
        &self,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), AppError> {
        // Deliberately self-only, with no admin override: an admin who could
        // set a member's password could then log in as them, which is exactly
        // the property ADR-0031 §3 rules out. An admin who needs to help a
        // member issues a fresh enrolment token instead.
        auth_context::require_authenticated()?;
        let user_id = Self::caller()?;
        validate_password(new_password)?;

        let existing = self
            .credentials
            .find_password(&user_id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Conflict(
                    "this account has no password to change; redeem an enrolment invitation"
                        .to_owned(),
                )
            })?;

        let secret = existing.secret.as_deref().ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "password credential {} has no secret",
                existing.id
            ))
        })?;
        crate::auth::password::verify_password(current_password, secret)?;

        let now = chrono::Utc::now().to_rfc3339();
        self.credentials
            .set_password(
                &existing.id,
                &user_id.to_string(),
                &existing.subject,
                &hash_password(new_password)?,
                &now,
            )
            .await
            .map_err(AppError::Internal)?;

        // Every session goes, including the caller's own. A password change is
        // most often a response to "somebody may know my password", and leaving
        // the attacker's session alive would make the change cosmetic.
        let removed = self
            .sessions
            .delete_all_for_user(&user_id.to_string())
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            user_id = %user_id,
            removed,
            "password changed, all sessions revoked: removed={removed}"
        );

        Ok(())
    }

    async fn issue_enrolment(&self, user_id: Uuid) -> Result<EnrolmentInvite, AppError> {
        auth_context::require_admin()?;

        let target = self.load(user_id).await?;
        if !target.enabled {
            return Err(AppError::Conflict(
                "cannot enrol a disabled user; enable the account first".to_owned(),
            ));
        }

        // Reuse the session-token generator: 32 bytes of CSPRNG, base64url,
        // stored only as a SHA-256 hash. The same reasoning applies — the token
        // has no guessable structure, so it needs no slow hash, and a readable
        // token in a backup would be a standing way in.
        let (token, token_hash) = new_session_token();
        let now = chrono::Utc::now();
        let expires_at = now + chrono::Duration::hours(ENROLMENT_TTL_HOURS);

        self.enrolments
            .create(&EnrolmentTokenRow {
                id: Uuid::new_v4().to_string(),
                user_id: user_id.to_string(),
                token_hash,
                created_at: now.to_rfc3339(),
                expires_at: expires_at.to_rfc3339(),
                used_at: None,
            })
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(user_id = %user_id, "enrolment invitation issued");

        Ok(EnrolmentInvite {
            token,
            expires_at: expires_at.to_rfc3339(),
            user_id,
        })
    }

    async fn list_enrolments(&self, user_id: Uuid) -> Result<Vec<EnrolmentSummary>, AppError> {
        auth_context::require_admin()?;

        self.enrolments
            .list_for_user(&user_id.to_string())
            .await
            .map_err(AppError::Internal)?
            .into_iter()
            .map(|row| {
                Ok(EnrolmentSummary {
                    id: Uuid::parse_str(&row.id).map_err(|e| {
                        AppError::Internal(anyhow::anyhow!("invalid enrolment id: {e}"))
                    })?,
                    user_id: Uuid::parse_str(&row.user_id).map_err(|e| {
                        AppError::Internal(anyhow::anyhow!("invalid user id: {e}"))
                    })?,
                    created_at: row.created_at,
                    expires_at: row.expires_at,
                    used_at: row.used_at,
                })
            })
            .collect()
    }

    async fn revoke_enrolment(&self, enrolment_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let removed = self
            .enrolments
            .delete(&enrolment_id.to_string())
            .await
            .map_err(AppError::Internal)?;
        if removed == 0 {
            return Err(AppError::NotFound("enrolment not found".to_owned()));
        }
        Ok(())
    }

    async fn redeem_enrolment(&self, token: &str, password: &str) -> Result<UserProfile, AppError> {
        // Documented exception to the auth-guard rule (`.agents/auth.md`
        // category (b), auth bootstrap): the caller has no credential yet, so
        // there is no session to require. The token is the authorization.
        validate_password(password)?;

        let now = chrono::Utc::now().to_rfc3339();
        let row = self
            .enrolments
            .find_redeemable(&hash_token(token), &now)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                // One message for unknown, expired and spent alike: telling a
                // caller which of the three it was turns this into an oracle
                // for guessing tokens.
                AppError::Unauthorized("that invitation is not valid".to_owned())
            })?;

        // Claim the token *before* writing the credential. `mark_used` carries
        // the `used_at IS NULL` predicate in its own WHERE clause, so two
        // concurrent redemptions cannot both win — and losing the race here
        // means no credential is written, which is the safe direction.
        let claimed = self
            .enrolments
            .mark_used(&row.id, &now)
            .await
            .map_err(AppError::Internal)?;
        if claimed == 0 {
            return Err(AppError::Unauthorized(
                "that invitation is not valid".to_owned(),
            ));
        }

        let user_id = Uuid::parse_str(&row.user_id)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid user id: {e}")))?;
        let user = self.load(user_id).await?;
        if !user.enabled {
            return Err(AppError::Forbidden("this account is disabled".to_owned()));
        }

        // The login subject is the user's email when they have one, and their id
        // otherwise. An account with no email can still be enrolled — it simply
        // has no human-typeable login until an email is set, which the admin UI
        // surfaces.
        let subject = user
            .email
            .clone()
            .unwrap_or_else(|| user.id.clone())
            .to_lowercase();

        self.credentials
            .set_password(
                &Uuid::new_v4().to_string(),
                &user.id,
                &subject,
                &hash_password(password)?,
                &now,
            )
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(user_id = %user_id, "enrolment redeemed, password set");

        UserProfile::from_row(user)
    }

    async fn cleanup_expired_enrolments(&self) -> Result<u64, AppError> {
        auth_context::require_admin()?;

        let now = chrono::Utc::now().to_rfc3339();
        self.enrolments
            .delete_expired(&now)
            .await
            .map_err(AppError::Internal)
    }
}
