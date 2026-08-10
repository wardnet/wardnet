//! The household user directory and enrolment (ADR-0031).

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::auth::UserRole;

use crate::auth::password::{hash_password, hash_token, new_session_token, validate_password};
use crate::auth_context;
use crate::error::AppError;
use crate::user::ceremony::CeremonyStore;
use crate::user::oauth::{
    OauthClient, OauthConfig, OauthProvider, PendingOauth, ProviderEndpoints, ProviderIdentity,
    ProviderStatus, new_pkce_pair, new_state,
};
use crate::user::passkey::{
    PasskeyMetadata, PasskeyRelyingParty, PendingAuthentication, PendingRegistration,
};
use wardnetd_data::repository::session::SessionRepository;
use wardnetd_data::repository::user::{DuplicateUserEmailError, UserRepository, UserRow};
use wardnetd_data::repository::user_credential::{
    CredentialAlreadyLinkedError, CredentialSummary, UserCredentialRepository,
};
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

    /// Which sign-in methods this box can actually offer right now.
    ///
    /// **Unauthenticated** — a documented exception (`.agents/auth.md`
    /// category (b)): it backs the sign-in surface, whose whole job is to be
    /// reachable before anybody has a session. It reports only whether a method
    /// is available, never any credential or client secret.
    async fn available_methods(&self) -> Result<AuthMethods, AppError>;

    /// Configure a provider's client id and secret.
    ///
    /// The secret goes to the `SecretStore` and is never readable back through
    /// any API; reads report `configured: true|false`. Passing `None` for the
    /// secret leaves an existing one in place, so an admin can toggle `enabled`
    /// or fix a typo'd client id without re-pasting it.
    async fn configure_oauth_provider(
        &self,
        provider: OauthProvider,
        client_id: &str,
        client_secret: Option<&str>,
        enabled: bool,
    ) -> Result<ProviderStatus, AppError>;

    /// Forget a provider's configuration entirely, including its secret.
    async fn clear_oauth_provider(&self, provider: OauthProvider) -> Result<(), AppError>;

    /// Begin an OAuth sign-in, returning the URL to send the browser to.
    ///
    /// **Unauthenticated** — this is a sign-in entry point (category (b)).
    async fn start_oauth(&self, provider: OauthProvider) -> Result<OauthRedirect, AppError>;

    /// Complete an OAuth sign-in.
    ///
    /// **Unauthenticated** (category (b)). Returns the user the provider
    /// account is linked to. An unknown subject is refused — Wardnet **never**
    /// auto-creates a household user from a federated login, because that would
    /// let anyone with a Google account create an account on somebody's home
    /// network. The admin links the account first.
    async fn complete_oauth(
        &self,
        state: &str,
        code: &str,
    ) -> Result<(UserProfile, UserRole), AppError>;

    /// Link a provider account to the calling user, completing a ceremony the
    /// caller started while signed in.
    async fn link_oauth(&self, state: &str, code: &str) -> Result<(), AppError>;

    /// Unlink every credential of one provider kind from a user.
    ///
    /// Never removes the local password: that is the floor, and a box whose
    /// only credential depended on a reachable provider would be unreachable
    /// during a WAN outage.
    async fn unlink_oauth(&self, user_id: Uuid, provider: OauthProvider) -> Result<u64, AppError>;

    /// Begin registering a passkey for the calling user.
    ///
    /// `request_host` is the `Host` the request arrived at. Returns `412` when
    /// passkeys cannot work here — no canonical hostname, or a host that is not
    /// the pinned Relying Party ID (ADR-0031 §8). The returned JSON is passed
    /// straight to `navigator.credentials.create()`.
    async fn start_passkey_registration(
        &self,
        request_host: &str,
    ) -> Result<serde_json::Value, AppError>;

    /// Finish registering a passkey.
    ///
    /// `label` names the credential in the list ("Pixel 8"). The response is the
    /// browser's `PublicKeyCredential`.
    async fn finish_passkey_registration(
        &self,
        request_host: &str,
        label: Option<&str>,
        credential: serde_json::Value,
    ) -> Result<(), AppError>;

    /// Begin a passkey sign-in.
    ///
    /// **Unauthenticated** (`.agents/auth.md` category (b)): a sign-in entry
    /// point. Discoverable credentials mean no username is supplied — the
    /// authenticator decides which passkey to offer.
    async fn start_passkey_authentication(
        &self,
        request_host: &str,
    ) -> Result<serde_json::Value, AppError>;

    /// Finish a passkey sign-in, returning the authenticated user.
    ///
    /// **Unauthenticated** (category (b)): this IS the credential check.
    async fn finish_passkey_authentication(
        &self,
        request_host: &str,
        credential: serde_json::Value,
    ) -> Result<(UserProfile, UserRole), AppError>;

    /// Delete every passkey in the household and unpin the Relying Party ID.
    ///
    /// The explicit admin recovery for an RP-ID divergence — a box that moved to
    /// a new hostname. Deliberately not automatic: silently re-pinning would
    /// invalidate every passkey with no explanation, and doing nothing would
    /// leave sign-in mysteriously broken. Local passwords are untouched, which
    /// is why this is recoverable at all.
    async fn reset_passkeys(&self) -> Result<u64, AppError>;
}

/// What the sign-in surface may render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthMethods {
    /// Always `true`. The local password is the floor: it works with no WAN, no
    /// certificate and no provider, and there is no path that removes it.
    pub password: bool,
    /// Per-provider availability and the redirect URI to register.
    pub providers: Vec<ProviderStatus>,
}

/// Where to send the browser to start an OAuth ceremony.
#[derive(Debug, Clone)]
pub struct OauthRedirect {
    /// The provider's authorize URL, fully parameterised.
    pub url: String,
}

/// Default [`UserService`] over the repository traits.
pub struct UserServiceImpl {
    users: Arc<dyn UserRepository>,
    credentials: Arc<dyn UserCredentialRepository>,
    enrolments: Arc<dyn UserEnrolmentRepository>,
    sessions: Arc<dyn SessionRepository>,
    oauth_config: OauthConfig,
    oauth_client: Arc<dyn OauthClient>,
    /// Open OAuth ceremonies. In memory and never persisted — see
    /// [`crate::user::ceremony`].
    pending_oauth: CeremonyStore<PendingOauth>,
    /// Provider endpoints, overridable so tests can point at `wiremock`.
    endpoints: Vec<(OauthProvider, ProviderEndpoints)>,
    passkeys: PasskeyRelyingParty,
    /// Open passkey registration ceremonies, keyed by challenge id.
    pending_registration: CeremonyStore<PendingRegistration>,
    /// Open passkey authentication ceremonies.
    pending_authentication: CeremonyStore<PendingAuthentication>,
}

impl UserServiceImpl {
    pub fn new(
        users: Arc<dyn UserRepository>,
        credentials: Arc<dyn UserCredentialRepository>,
        enrolments: Arc<dyn UserEnrolmentRepository>,
        sessions: Arc<dyn SessionRepository>,
        oauth_config: OauthConfig,
        oauth_client: Arc<dyn OauthClient>,
    ) -> Self {
        let passkeys = PasskeyRelyingParty::new(oauth_config.system_config.clone());
        Self {
            users,
            credentials,
            enrolments,
            sessions,
            oauth_config,
            oauth_client,
            passkeys,
            pending_registration: CeremonyStore::new(),
            pending_authentication: CeremonyStore::new(),
            pending_oauth: CeremonyStore::new(),
            endpoints: vec![
                (
                    OauthProvider::Google,
                    ProviderEndpoints::production(OauthProvider::Google),
                ),
                (
                    OauthProvider::Github,
                    ProviderEndpoints::production(OauthProvider::Github),
                ),
            ],
        }
    }

    /// Replace one provider's endpoints. Test-facing: production always uses
    /// [`ProviderEndpoints::production`].
    #[must_use]
    pub fn with_endpoints(mut self, provider: OauthProvider, endpoints: ProviderEndpoints) -> Self {
        self.endpoints.retain(|(p, _)| *p != provider);
        self.endpoints.push((provider, endpoints));
        self
    }

    /// The endpoints in force for `provider`.
    fn endpoints_for(&self, provider: OauthProvider) -> ProviderEndpoints {
        self.endpoints
            .iter()
            .find(|(p, _)| *p == provider)
            .map(|(_, e)| e.clone())
            .unwrap_or_else(|| ProviderEndpoints::production(provider))
    }

    /// Everything needed to talk to a provider, or a clear refusal.
    ///
    /// Checked in one place so no path can reach a provider call with a missing
    /// piece — and so the failure is `Conflict` ("not configured"), never a
    /// confusing 500 from an empty client id.
    async fn provider_ready(
        &self,
        provider: OauthProvider,
    ) -> Result<(String, String, String), AppError> {
        let status = self.oauth_config.status(provider).await?;
        if !status.enabled {
            return Err(AppError::Conflict(format!(
                "{} sign-in is not configured on this box",
                provider.as_str()
            )));
        }
        let redirect_uri = status.redirect_uri.ok_or_else(|| {
            AppError::Conflict(
                "this box has no canonical public hostname, so federated sign-in \
                 cannot work; set up remote access first"
                    .to_owned(),
            )
        })?;
        let client_id = self
            .oauth_config
            .client_id(provider)
            .await?
            .ok_or_else(|| AppError::Conflict("no client id configured".to_owned()))?;
        let client_secret = self
            .oauth_config
            .client_secret(provider)
            .await?
            .ok_or_else(|| AppError::Conflict("no client secret configured".to_owned()))?;
        Ok((client_id, client_secret, redirect_uri))
    }

    /// Run an OAuth callback to the point of knowing who the provider says this
    /// is. Shared by sign-in and account-linking, which differ only in what they
    /// do with the answer.
    async fn resolve_callback(
        &self,
        state: &str,
        code: &str,
    ) -> Result<(OauthProvider, ProviderIdentity, Option<Uuid>), AppError> {
        // `take` removes the entry, so a replayed `state` fails here even if the
        // rest of the ceremony would have succeeded.
        let pending = self.pending_oauth.take(state).ok_or_else(|| {
            AppError::Unauthorized("that sign-in attempt is not valid; please try again".to_owned())
        })?;

        let provider = pending.provider;
        let (client_id, client_secret, redirect_uri) = self.provider_ready(provider).await?;

        let identity = self
            .oauth_client
            .exchange_and_identify(
                provider,
                &self.endpoints_for(provider),
                &client_id,
                &client_secret,
                &redirect_uri,
                code,
                &pending.pkce_verifier,
            )
            .await?;

        Ok((provider, identity, pending.started_by))
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
            return AppError::Conflict(
                "a household user with that email already exists".to_owned(),
            );
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

    /// The `webauthn-rs` credential ids of a user's existing passkeys.
    ///
    /// Passed as the exclude-list at registration so an authenticator offers to
    /// update an existing passkey rather than silently creating a second one for
    /// the same account on the same device.
    async fn user_passkey_ids(
        &self,
        user_id: &str,
    ) -> Result<Vec<webauthn_rs::prelude::CredentialID>, AppError> {
        let summaries = self
            .credentials
            .list_for_user(user_id)
            .await
            .map_err(AppError::Internal)?;

        Ok(summaries
            .into_iter()
            .filter(|c| {
                c.kind == wardnetd_data::repository::user_credential::CredentialKind::Passkey
            })
            .filter_map(|c| {
                use base64::Engine;
                base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(&c.subject)
                    .ok()
                    .map(Into::into)
            })
            .collect())
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

        // The email **is** the password login identifier, so changing it has to
        // move the credential's `subject` too. Without that the two silently
        // diverge: `/api/users/me` shows the new address while only the retired
        // one authenticates, and the freed address becomes a landmine — a later
        // user given it trips `UNIQUE(kind, subject)` on enrolment.
        //
        // Only the password credential is touched. A passkey's subject is its
        // credential id and an OAuth link's is the provider's subject; neither
        // has anything to do with the email.
        //
        // The credential is written **first**, deliberately. There is no
        // transaction across these two rows, so one order has to be chosen for
        // its failure mode: if the credential write fails here, nothing has
        // changed at all. Writing `users` first and failing here would leave the
        // profile showing an address that cannot log in — a lockout.
        if let Some(existing) = self
            .credentials
            .find_password(&user_id.to_string())
            .await
            .map_err(AppError::Internal)?
        {
            let new_subject = email
                .clone()
                .unwrap_or_else(|| user_id.to_string())
                .to_lowercase();

            if new_subject != existing.subject {
                let secret = existing.secret.as_deref().ok_or_else(|| {
                    AppError::Internal(anyhow::anyhow!(
                        "password credential {} has no secret",
                        existing.id
                    ))
                })?;

                // Re-points the subject and keeps the same hash — a person's
                // password does not change because their address did.
                if let Err(e) = self
                    .credentials
                    .set_password(
                        &existing.id,
                        &user_id.to_string(),
                        &new_subject,
                        secret,
                        &now,
                    )
                    .await
                {
                    if e.downcast_ref::<CredentialAlreadyLinkedError>().is_some() {
                        return Err(AppError::Conflict(
                            "another account already signs in with that email address".to_owned(),
                        ));
                    }
                    return Err(AppError::Internal(e));
                }
            }
        }

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

        // Refuse up front for an account that already has a password, so the
        // admin sees the problem when they ask rather than handing over a token
        // that `redeem_enrolment` will refuse. That refusal is the real guard —
        // this one exists so the UI can say why, and so a token that could
        // never be spent is never minted.
        if self
            .credentials
            .find_password(&target.id)
            .await
            .map_err(AppError::Internal)?
            .is_some()
        {
            return Err(AppError::Conflict(
                "that account already has a password; enrolment sets a first \
                 credential and never replaces one"
                    .to_owned(),
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
                    user_id: Uuid::parse_str(&row.user_id)
                        .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid user id: {e}")))?,
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

        let user_id = Uuid::parse_str(&row.user_id)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid user id: {e}")))?;
        let user = self.load(user_id).await?;
        if !user.enabled {
            return Err(AppError::Forbidden("this account is disabled".to_owned()));
        }

        // An enrolment token sets a *first* credential; it must never replace an
        // existing one. Without this, an admin could issue a token against an
        // account that already has a password, redeem it with a password of
        // their choosing, and then sign in as that person — which is exactly the
        // property ADR-0031 §3 rules out, and the reason
        // `change_own_password` has no admin override.
        if self
            .credentials
            .find_password(&user.id)
            .await
            .map_err(AppError::Internal)?
            .is_some()
        {
            tracing::warn!(
                user_id = %user_id,
                "refused an enrolment redemption against an account that already has a password"
            );
            return Err(AppError::Conflict(
                "this account already has a password; use the change-password flow instead"
                    .to_owned(),
            ));
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

        // Hash before claiming the token: Argon2id is the slowest step and the
        // most likely to be interrupted, and a token burned with no credential
        // written costs the household a fresh invitation.
        let secret = hash_password(password)?;

        // Claim the token only once everything else has succeeded. `mark_used`
        // carries the `used_at IS NULL` predicate in its own WHERE clause, so
        // two concurrent redemptions still cannot both win — losing that race
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

        // The subject collides only if another account already logs in with this
        // email — a real conflict the admin has to resolve, and a `409` rather
        // than the `500` a bare downcast-less map would produce.
        if let Err(e) = self
            .credentials
            .set_password(
                &Uuid::new_v4().to_string(),
                &user.id,
                &subject,
                &secret,
                &now,
            )
            .await
        {
            if e.downcast_ref::<CredentialAlreadyLinkedError>().is_some() {
                return Err(AppError::Conflict(
                    "another account already signs in with that email address".to_owned(),
                ));
            }
            return Err(AppError::Internal(e));
        }

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

    async fn available_methods(&self) -> Result<AuthMethods, AppError> {
        // Documented exception to the auth-guard rule (`.agents/auth.md`
        // category (b)): this backs the sign-in surface, which by definition is
        // reached before anybody has a session. It returns availability only.
        Ok(AuthMethods {
            // Not a stored flag. The local password is the floor by
            // construction — there is no code path that removes it — so
            // reporting it from configuration would invite it to become false.
            password: true,
            providers: vec![
                self.oauth_config.status(OauthProvider::Google).await?,
                self.oauth_config.status(OauthProvider::Github).await?,
            ],
        })
    }

    async fn configure_oauth_provider(
        &self,
        provider: OauthProvider,
        client_id: &str,
        client_secret: Option<&str>,
        enabled: bool,
    ) -> Result<ProviderStatus, AppError> {
        auth_context::require_admin()?;

        let client_id = client_id.trim();
        if client_id.is_empty() {
            return Err(AppError::BadRequest(
                "client id must not be empty".to_owned(),
            ));
        }

        self.oauth_config
            .system_config
            .set(&provider.client_id_key(), client_id)
            .await
            .map_err(AppError::Internal)?;

        // `None` keeps the existing secret, so an admin can flip `enabled` or
        // correct a client id without re-pasting a value they may not still
        // have. An empty string is a mistake, not "clear it" — that is what
        // `clear_oauth_provider` is for.
        if let Some(secret) = client_secret {
            if secret.trim().is_empty() {
                return Err(AppError::BadRequest(
                    "client secret must not be empty; use the clear action to remove it".to_owned(),
                ));
            }
            self.oauth_config
                .secrets
                .put(&provider.secret_path(), secret.trim().as_bytes())
                .await
                .map_err(AppError::Internal)?;
        }

        self.oauth_config
            .system_config
            .set(
                &provider.enabled_key(),
                if enabled { "true" } else { "false" },
            )
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            provider = provider.as_str(),
            enabled,
            "oauth provider configured: provider={}",
            provider.as_str()
        );

        self.oauth_config.status(provider).await
    }

    async fn clear_oauth_provider(&self, provider: OauthProvider) -> Result<(), AppError> {
        auth_context::require_admin()?;

        self.oauth_config
            .system_config
            .delete(&provider.client_id_key())
            .await
            .map_err(AppError::Internal)?;
        self.oauth_config
            .system_config
            .set(&provider.enabled_key(), "false")
            .await
            .map_err(AppError::Internal)?;
        self.oauth_config
            .secrets
            .delete(&provider.secret_path())
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            provider = provider.as_str(),
            "oauth provider configuration cleared: provider={}",
            provider.as_str()
        );
        Ok(())
    }

    async fn start_oauth(&self, provider: OauthProvider) -> Result<OauthRedirect, AppError> {
        // Documented exception (category (b)): a sign-in entry point.
        let (client_id, _secret, redirect_uri) = self.provider_ready(provider).await?;
        let endpoints = self.endpoints_for(provider);

        let state = new_state();
        let (verifier, challenge) = new_pkce_pair();

        // A ceremony started by a signed-in user is a *link*; record who, so
        // `link_oauth` can refuse a `state` minted by somebody else. Sign-in
        // ceremonies have no caller, which is the `None` case.
        let started_by = auth_context::try_current().and_then(|ctx| ctx.user_id());

        self.pending_oauth.insert(
            state.clone(),
            PendingOauth {
                provider,
                pkce_verifier: verifier,
                started_by,
            },
        );

        // Built through `Url` rather than string interpolation: a client id or
        // scope containing a reserved character would otherwise silently corrupt
        // the query. `reqwest` re-exports this, so it costs no new dependency.
        let mut url = reqwest::Url::parse(&endpoints.authorize_url).map_err(|e| {
            AppError::Internal(anyhow::anyhow!("invalid authorize url configured: {e}"))
        })?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("scope", &endpoints.scopes)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");

        Ok(OauthRedirect {
            url: url.to_string(),
        })
    }

    async fn complete_oauth(
        &self,
        state: &str,
        code: &str,
    ) -> Result<(UserProfile, UserRole), AppError> {
        // Documented exception (category (b)): this IS the credential check.
        let (provider, identity, _started_by) = self.resolve_callback(state, code).await?;

        // The subject is the join key, and `find_for_login` filters disabled
        // users in SQL. An unknown subject is refused outright: Wardnet never
        // auto-creates a household user from a federated login, because that
        // would let anybody with a Google account create an account on somebody
        // else's home network.
        let login = self
            .credentials
            .find_for_login(provider.credential_kind(), &identity.subject)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                tracing::warn!(
                    provider = provider.as_str(),
                    "oauth sign-in refused: provider account is not linked to any household user"
                );
                AppError::Unauthorized(
                    "that account is not linked to a household user; ask an admin to link it"
                        .to_owned(),
                )
            })?;

        let now = chrono::Utc::now().to_rfc3339();
        if let Err(e) = self
            .credentials
            .touch_last_used(&login.credential.id, &now)
            .await
        {
            tracing::warn!(error = %e, "failed to record credential last_used_at");
        }

        let user_id = Uuid::parse_str(&login.credential.user_id)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid user id: {e}")))?;
        let profile = UserProfile::from_row(self.load(user_id).await?)?;
        let role = login.role;

        tracing::info!(
            provider = provider.as_str(),
            user_id = %user_id,
            "oauth sign-in succeeded: provider={}",
            provider.as_str()
        );

        Ok((profile, role))
    }

    async fn link_oauth(&self, state: &str, code: &str) -> Result<(), AppError> {
        auth_context::require_authenticated()?;
        let user_id = Self::caller()?;

        let (provider, identity, started_by) = self.resolve_callback(state, code).await?;

        // The ceremony must belong to the caller. Without this, an attacker
        // could start a ceremony with their own provider account, obtain
        // `(state, code)`, and get a signed-in admin's browser to redeem it —
        // attaching the attacker's account to the admin's user and handing them
        // the household. `finish_passkey_registration` refuses on the same
        // mismatch.
        //
        // A `None` owner means the ceremony was started unauthenticated, i.e.
        // as a sign-in. Those are not linkable, so refuse rather than adopt.
        match started_by {
            Some(owner) if owner == user_id => {}
            _ => {
                tracing::warn!(
                    user_id = %user_id,
                    "refused an oauth link for a ceremony this caller did not start"
                );
                return Err(AppError::Forbidden(
                    "that sign-in attempt was not started by this account".to_owned(),
                ));
            }
        }

        let now = chrono::Utc::now().to_rfc3339();
        let row = wardnetd_data::repository::user_credential::CredentialRow {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            kind: provider.credential_kind(),
            subject: identity.subject,
            // No secret: the provider holds the credential. What is stored is
            // only the link.
            secret: None,
            label: identity.label,
            metadata: "{}".to_owned(),
            created_at: now,
            last_used_at: None,
        };

        self.credentials.insert(&row).await.map_err(|e| {
            // The `(kind, subject)` uniqueness constraint is the anti-hijack
            // invariant: one provider account links to at most one household
            // user. The refusal deliberately does not say *which* user already
            // holds the link — that would turn a link attempt into a
            // directory-enumeration oracle.
            if e.downcast_ref::<wardnetd_data::repository::user_credential::CredentialAlreadyLinkedError>()
                .is_some()
            {
                return AppError::Conflict(
                    "that provider account is already linked to an account".to_owned(),
                );
            }
            AppError::Internal(e)
        })?;

        tracing::info!(
            provider = provider.as_str(),
            user_id = %user_id,
            "oauth account linked: provider={}",
            provider.as_str()
        );
        Ok(())
    }

    async fn unlink_oauth(&self, user_id: Uuid, provider: OauthProvider) -> Result<u64, AppError> {
        Self::require_admin_or_self(user_id)?;

        // Only ever removes federated links. The local password is not
        // reachable from here at all: a box whose sole credential depended on a
        // reachable provider would be unreachable during a WAN outage, which is
        // exactly what ADR-0031 refuses.
        let removed = self
            .credentials
            .delete_by_kind(&user_id.to_string(), provider.credential_kind())
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            provider = provider.as_str(),
            user_id = %user_id,
            removed,
            "oauth account unlinked: removed={removed}"
        );
        Ok(removed)
    }

    async fn start_passkey_registration(
        &self,
        request_host: &str,
    ) -> Result<serde_json::Value, AppError> {
        auth_context::require_authenticated()?;
        let user_id = Self::caller()?;
        let user = self.load(user_id).await?;

        let fqdn = self.oauth_config.canonical_fqdn().await?;
        let webauthn = self
            .passkeys
            .for_request(fqdn.as_deref(), request_host)
            .await?;

        // Existing passkeys are excluded so the authenticator offers to *update*
        // rather than silently creating a second credential for the same user on
        // the same device.
        let existing = self.user_passkey_ids(&user.id).await?;

        let (challenge, state) = webauthn
            .start_passkey_registration(
                webauthn_rs::prelude::Uuid::from_bytes(*user_id.as_bytes()),
                // The account name the authenticator shows. Email when we have
                // one, because that is what a person recognises in a passkey
                // picker; the display name otherwise.
                user.email.as_deref().unwrap_or(&user.display_name),
                &user.display_name,
                Some(existing),
            )
            .map_err(map_webauthn_error)?;

        let challenge_id = new_state();
        self.pending_registration
            .insert(challenge_id.clone(), PendingRegistration { user_id, state });

        // The challenge id rides along in the response so the browser can hand
        // it back; the ceremony state itself never leaves the daemon.
        let mut value = serde_json::to_value(&challenge).map_err(|e| {
            AppError::Internal(anyhow::anyhow!("failed to serialise challenge: {e}"))
        })?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "challengeId".to_owned(),
                serde_json::Value::String(challenge_id),
            );
        }
        Ok(value)
    }

    async fn finish_passkey_registration(
        &self,
        request_host: &str,
        label: Option<&str>,
        credential: serde_json::Value,
    ) -> Result<(), AppError> {
        auth_context::require_authenticated()?;
        let user_id = Self::caller()?;

        let (challenge_id, credential) = split_challenge_id(credential)?;
        let pending = self
            .pending_registration
            .take(&challenge_id)
            .ok_or_else(|| {
                AppError::Unauthorized(
                    "that passkey registration is no longer valid; please try again".to_owned(),
                )
            })?;

        // The ceremony belongs to whoever started it. Without this, a caller
        // could complete somebody else's registration and attach the credential
        // to their own account.
        if pending.user_id != user_id {
            return Err(AppError::Forbidden(
                "that registration was started by a different user".to_owned(),
            ));
        }

        let fqdn = self.oauth_config.canonical_fqdn().await?;
        let webauthn = self
            .passkeys
            .for_request(fqdn.as_deref(), request_host)
            .await?;

        let registration: webauthn_rs::prelude::RegisterPublicKeyCredential =
            serde_json::from_value(credential).map_err(|e| {
                AppError::BadRequest(format!("malformed passkey registration response: {e}"))
            })?;

        let passkey = webauthn
            .finish_passkey_registration(&registration, &pending.state)
            .map_err(map_webauthn_error)?;

        let metadata = PasskeyMetadata {
            // A fresh credential has no observed assertion yet. The backup flags
            // are not knowable here (see `PasskeyMetadata`) and are filled in on
            // first sign-in rather than guessed.
            sign_count: 0,
            backup_eligible: None,
            backup_state: None,
            credential: passkey.clone(),
        };

        let now = chrono::Utc::now().to_rfc3339();
        let row = wardnetd_data::repository::user_credential::CredentialRow {
            id: Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            kind: wardnetd_data::repository::user_credential::CredentialKind::Passkey,
            // The base64url credential id is the login key, which is what makes
            // a discoverable sign-in resolvable without a username.
            subject: base64_url(passkey.cred_id().as_ref()),
            // The COSE public key lives in `metadata`, not `secret`: it is a
            // public key, and putting it in `secret` would imply it needs the
            // same handling as a password hash.
            secret: None,
            label: label.map(str::to_owned),
            metadata: serde_json::to_string(&metadata).map_err(|e| {
                AppError::Internal(anyhow::anyhow!("failed to serialise passkey: {e}"))
            })?,
            created_at: now,
            last_used_at: None,
        };

        self.credentials.insert(&row).await.map_err(|e| {
            if e.downcast_ref::<wardnetd_data::repository::user_credential::CredentialAlreadyLinkedError>()
                .is_some()
            {
                return AppError::Conflict("that passkey is already registered".to_owned());
            }
            AppError::Internal(e)
        })?;

        tracing::info!(user_id = %user_id, "passkey registered");
        Ok(())
    }

    async fn start_passkey_authentication(
        &self,
        request_host: &str,
    ) -> Result<serde_json::Value, AppError> {
        // Documented exception (category (b)): a sign-in entry point.
        let fqdn = self.oauth_config.canonical_fqdn().await?;
        let webauthn = self
            .passkeys
            .for_request(fqdn.as_deref(), request_host)
            .await?;

        // Discoverable credentials: no allow-list, so the authenticator decides
        // which passkey to offer and no username is needed. That also means this
        // request discloses nothing about which accounts exist.
        let (challenge, state) = webauthn
            .start_discoverable_authentication()
            .map_err(map_webauthn_error)?;

        let challenge_id = new_state();
        self.pending_authentication
            .insert(challenge_id.clone(), PendingAuthentication { state });

        let mut value = serde_json::to_value(&challenge).map_err(|e| {
            AppError::Internal(anyhow::anyhow!("failed to serialise challenge: {e}"))
        })?;
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "challengeId".to_owned(),
                serde_json::Value::String(challenge_id),
            );
        }
        Ok(value)
    }

    async fn finish_passkey_authentication(
        &self,
        request_host: &str,
        credential: serde_json::Value,
    ) -> Result<(UserProfile, UserRole), AppError> {
        // Documented exception (category (b)): this IS the credential check.
        let (challenge_id, credential) = split_challenge_id(credential)?;
        let pending = self
            .pending_authentication
            .take(&challenge_id)
            .ok_or_else(|| {
                AppError::Unauthorized(
                    "that sign-in attempt is no longer valid; please try again".to_owned(),
                )
            })?;

        let fqdn = self.oauth_config.canonical_fqdn().await?;
        let webauthn = self
            .passkeys
            .for_request(fqdn.as_deref(), request_host)
            .await?;

        let assertion: webauthn_rs::prelude::PublicKeyCredential =
            serde_json::from_value(credential)
                .map_err(|e| AppError::BadRequest(format!("malformed passkey assertion: {e}")))?;

        // Resolve which credential the authenticator used, then look it up. The
        // browser tells us the credential id; the signature is what proves it.
        let cred_id = base64_url(assertion.raw_id.as_ref());
        let login = self
            .credentials
            .find_for_login(
                wardnetd_data::repository::user_credential::CredentialKind::Passkey,
                &cred_id,
            )
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Unauthorized("that passkey is not registered here".to_owned())
            })?;

        let mut metadata: PasskeyMetadata = serde_json::from_str(&login.credential.metadata)
            .map_err(|e| {
                AppError::Internal(anyhow::anyhow!(
                    "stored passkey metadata is unreadable: {e}"
                ))
            })?;

        let result = webauthn
            .finish_discoverable_authentication(
                &assertion,
                pending.state,
                &[(&metadata.credential).into()],
            )
            .map_err(map_webauthn_error)?;

        // A counter that went backwards is the signal of a cloned credential.
        // `webauthn-rs` reports it; refusing and logging loudly is the whole
        // point of persisting the counter at all.
        if result.counter() > 0 && result.counter() < metadata.sign_count {
            tracing::error!(
                user_id = %login.credential.user_id,
                stored = metadata.sign_count,
                presented = result.counter(),
                "passkey signature counter regressed; refusing the sign-in"
            );
            return Err(AppError::Unauthorized(
                "that passkey failed verification".to_owned(),
            ));
        }

        // Write the counter and backup state back, so the next check has
        // something current to compare against.
        metadata.credential.update_credential(&result);
        metadata.sign_count = result.counter();
        metadata.backup_state = Some(result.backup_state());
        metadata.backup_eligible = Some(result.backup_eligible());
        let now = chrono::Utc::now().to_rfc3339();
        if let Ok(json) = serde_json::to_string(&metadata) {
            if let Err(e) = self
                .credentials
                .update_metadata(&login.credential.id, &json)
                .await
            {
                tracing::warn!(error = %e, "failed to persist passkey counter");
            }
        }
        if let Err(e) = self
            .credentials
            .touch_last_used(&login.credential.id, &now)
            .await
        {
            tracing::warn!(error = %e, "failed to record credential last_used_at");
        }

        let user_id = Uuid::parse_str(&login.credential.user_id)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid user id: {e}")))?;
        let profile = UserProfile::from_row(self.load(user_id).await?)?;

        tracing::info!(user_id = %user_id, "passkey sign-in succeeded");
        Ok((profile, login.role))
    }

    async fn reset_passkeys(&self) -> Result<u64, AppError> {
        auth_context::require_admin()?;

        // Every user's passkeys, not just the caller's: an RP-ID divergence
        // breaks them all, so a per-user reset would leave the household half
        // broken and confused.
        let mut removed = 0;
        for user in self.users.find_all().await.map_err(AppError::Internal)? {
            removed += self
                .credentials
                .delete_by_kind(
                    &user.id,
                    wardnetd_data::repository::user_credential::CredentialKind::Passkey,
                )
                .await
                .map_err(AppError::Internal)?;
        }

        // Unpin last. If this failed with the credentials already gone the box
        // is still usable (passwords are untouched) and the next registration
        // re-pins; the reverse order could leave passkeys pinned to a hostname
        // that no longer exists.
        self.passkeys.unpin().await?;

        tracing::warn!(removed, "passkeys reset: removed={removed}");
        Ok(removed)
    }
}

/// Base64url-encode without padding — how a `WebAuthn` credential id is written.
fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Split the `challengeId` the daemon added out of a browser response.
///
/// The ceremony state lives server-side; only this opaque handle round-trips, so
/// nothing about the challenge is client-controlled.
fn split_challenge_id(
    mut value: serde_json::Value,
) -> Result<(String, serde_json::Value), AppError> {
    let id = value
        .as_object_mut()
        .and_then(|o| o.remove("challengeId"))
        .and_then(|v| v.as_str().map(str::to_owned))
        .ok_or_else(|| AppError::BadRequest("missing challengeId".to_owned()))?;
    Ok((id, value))
}

/// Map a `webauthn-rs` failure to an `AppError`.
///
/// Every verification failure is `Unauthorized` with one message. The library
/// distinguishes many causes — wrong origin, bad signature, failed attestation —
/// and reporting which would tell an attacker exactly which part of a forged
/// assertion to fix. The detail goes to the log instead.
fn map_webauthn_error(err: webauthn_rs::prelude::WebauthnError) -> AppError {
    tracing::warn!(error = %err, "webauthn ceremony failed: {err}");
    AppError::Unauthorized("that passkey failed verification".to_owned())
}
