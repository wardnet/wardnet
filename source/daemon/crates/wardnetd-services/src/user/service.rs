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
    ProviderStatus, ReturnTo, new_pkce_pair, new_state,
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

    /// Begin an OAuth ceremony, returning the URL to send the browser to.
    ///
    /// A ceremony started by a signed-in caller is a **link**; one started
    /// anonymously is a **sign-in**. `return_to` and `remember_me` are the
    /// caller's intent, parked on the ceremony because the callback that
    /// consumes them is a bare provider redirect with no body (ADR-0031 §11).
    ///
    /// **Unauthenticated** — this is a sign-in entry point (category (b)).
    async fn start_oauth(
        &self,
        provider: OauthProvider,
        return_to: ReturnTo,
        remember_me: bool,
    ) -> Result<OauthRedirect, AppError>;

    /// Complete an OAuth ceremony — **the only callback entry point**.
    ///
    /// One registered redirect URI serves both a sign-in and a link, and the
    /// request arriving at it carries nothing that says which. The ceremony
    /// does: `started_by` is `None` for a sign-in and `Some(user)` for a link.
    /// Dispatching here rather than in the HTTP handler is not a style choice —
    /// resolving a callback **consumes** the single-use `state` before a
    /// handler could discover it guessed wrong, so a guess that misses leaves
    /// the person with a spent ceremony and no way forward (ADR-0031 §11).
    ///
    /// **Unauthenticated** (category (b)): this *is* the credential check. The
    /// link arm additionally requires that the caller is the user who started
    /// the ceremony, which it enforces itself after resolving.
    ///
    /// An unknown subject is refused — Wardnet **never** auto-creates a
    /// household user from a federated login, because that would let anyone
    /// with a Google account create an account on somebody's home network. An
    /// admin links the account first.
    async fn complete_oauth_callback(
        &self,
        state: &str,
        code: &str,
    ) -> Result<OauthOutcome, AppError>;

    /// Unlink every credential of one provider kind from a user.
    ///
    /// Never removes the local password: that is the floor, and a box whose
    /// only credential depended on a reachable provider would be unreachable
    /// during a WAN outage.
    async fn unlink_oauth(&self, user_id: Uuid, provider: OauthProvider) -> Result<u64, AppError>;
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

/// What a resolved OAuth callback turned out to be.
///
/// The two ceremonies share one registered redirect URI, so the callback cannot
/// know which it is handling until the ceremony has been read. Returning a
/// typed outcome — rather than letting the caller pick an entry point up front
/// and hope — is what makes the wrong guess unrepresentable (ADR-0031 §11).
#[derive(Debug, Clone)]
pub enum OauthOutcome {
    /// A sign-in: the provider account is linked to this user, who is enabled.
    ///
    /// Carries no session. Minting one is [`AuthService`]'s job, because that
    /// is where session policy lives; the caller passes `return_to` and
    /// `remember_me` straight through from the ceremony.
    ///
    /// [`AuthService`]: crate::auth::AuthService
    SignedIn {
        profile: UserProfile,
        role: UserRole,
        return_to: ReturnTo,
        remember_me: bool,
    },
    /// A link: a provider account was attached to the caller's own user.
    Linked {
        user_id: Uuid,
        provider: OauthProvider,
        return_to: ReturnTo,
    },
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
        Self {
            users,
            credentials,
            enrolments,
            sessions,
            oauth_config,
            oauth_client,
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
            .map_or_else(
                || ProviderEndpoints::production(provider),
                |(_, e)| e.clone(),
            )
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

        // A missing hostname is checked before the blanket `enabled` flag
        // because `enabled` already folds `fqdn.is_some()` in: reporting "not
        // configured" to an admin who filled the provider form in correctly
        // sends them back to the wrong screen. Only say that when they have in
        // fact not configured it.
        let Some(redirect_uri) = status.redirect_uri else {
            return Err(AppError::Conflict(if status.configured {
                "this box has no canonical public hostname, so federated sign-in \
                 cannot work; set up remote access first"
                    .to_owned()
            } else {
                format!(
                    "{} sign-in is not configured on this box",
                    provider.as_str()
                )
            }));
        };
        if !status.enabled {
            return Err(AppError::Conflict(format!(
                "{} sign-in is not configured on this box",
                provider.as_str()
            )));
        }
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
    ) -> Result<
        (
            OauthProvider,
            ProviderIdentity,
            Option<Uuid>,
            ReturnTo,
            bool,
        ),
        AppError,
    > {
        // `take` removes the entry, so a replayed `state` fails here even if the
        // rest of the ceremony would have succeeded.
        let pending = self.pending_oauth.take(state).ok_or_else(|| {
            AppError::Unauthorized("that sign-in attempt is not valid; please try again".to_owned())
        })?;

        let provider = pending.provider;
        let (client_id, client_secret, redirect_uri) = self.provider_ready(provider).await?;

        let identity = self
            .oauth_client
            .exchange_and_identify(crate::user::oauth::OauthExchange {
                provider,
                endpoints: &self.endpoints_for(provider),
                client_id: &client_id,
                client_secret: &client_secret,
                redirect_uri: &redirect_uri,
                code,
                pkce_verifier: &pending.pkce_verifier,
            })
            .await?;

        Ok((
            provider,
            identity,
            pending.started_by,
            pending.return_to,
            pending.remember_me,
        ))
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

    async fn start_oauth(
        &self,
        provider: OauthProvider,
        return_to: ReturnTo,
        remember_me: bool,
    ) -> Result<OauthRedirect, AppError> {
        // Documented exception (category (b)): a sign-in entry point.
        let (client_id, _secret, redirect_uri) = self.provider_ready(provider).await?;
        let endpoints = self.endpoints_for(provider);

        let state = new_state();
        let (verifier, challenge) = new_pkce_pair();

        // A ceremony started by a signed-in user is a *link*; record who, so
        // the callback can both route to the link arm and refuse a `state`
        // minted by somebody else. Sign-in ceremonies have no caller, which is
        // the `None` case — and that same field is what the single callback
        // dispatches on (ADR-0031 §11).
        let started_by = auth_context::try_current().and_then(|ctx| ctx.user_id());

        self.pending_oauth.insert(
            state.clone(),
            PendingOauth {
                provider,
                pkce_verifier: verifier,
                started_by,
                return_to,
                remember_me,
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

    async fn complete_oauth_callback(
        &self,
        state: &str,
        code: &str,
    ) -> Result<OauthOutcome, AppError> {
        // Documented exception (category (b)): this IS the credential check.
        // The link arm guards itself once the ceremony has been read.
        //
        // Resolving consumes the `state`, so the ceremony kind has to be
        // decided from what comes back — never guessed beforehand by a caller
        // who would have no way to undo a wrong guess (ADR-0031 §11).
        let (provider, identity, started_by, return_to, remember_me) =
            self.resolve_callback(state, code).await?;

        match started_by {
            None => {
                self.sign_in_with(provider, &identity, return_to, remember_me)
                    .await
            }
            Some(owner) => self.link_for(provider, identity, owner, return_to).await,
        }
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
}

impl UserServiceImpl {
    /// The sign-in arm of a resolved callback.
    async fn sign_in_with(
        &self,
        provider: OauthProvider,
        identity: &ProviderIdentity,
        return_to: ReturnTo,
        remember_me: bool,
    ) -> Result<OauthOutcome, AppError> {
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

        Ok(OauthOutcome::SignedIn {
            profile,
            role,
            return_to,
            remember_me,
        })
    }

    /// The link arm of a resolved callback.
    ///
    /// `owner` is the ceremony's `started_by`, already known to be `Some`.
    /// The caller must *be* that user: without the check, an attacker could
    /// start a ceremony with their own provider account, obtain
    /// `(state, code)`, and get a signed-in admin's browser to redeem it —
    /// attaching the attacker's account to the admin's user and handing them
    /// the household.
    async fn link_for(
        &self,
        provider: OauthProvider,
        identity: ProviderIdentity,
        owner: Uuid,
        return_to: ReturnTo,
    ) -> Result<OauthOutcome, AppError> {
        // A link completes as the caller, so the caller must be authenticated
        // and must be the person who started it. The sign-in arm has no such
        // requirement, which is why this guard lives here rather than on the
        // shared entry point.
        auth_context::require_authenticated()?;
        let user_id = Self::caller()?;

        if owner != user_id {
            tracing::warn!(
                user_id = %user_id,
                "refused an oauth link for a ceremony this caller did not start"
            );
            return Err(AppError::Forbidden(
                "that sign-in attempt was not started by this account".to_owned(),
            ));
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
        Ok(OauthOutcome::Linked {
            user_id,
            provider,
            return_to,
        })
    }
}
