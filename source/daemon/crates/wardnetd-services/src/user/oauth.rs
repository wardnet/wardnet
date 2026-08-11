//! Google and `GitHub` sign-in (ADR-0031 §2).
//!
//! # Each household registers its own app
//!
//! Wardnet ships no client credentials. The admin creates an OAuth app with the
//! provider and pastes the id and secret in; Wardnet shows the exact redirect
//! URI to register. The alternative — a Wardnet-hosted callback that then
//! redirects to the box — would put a third party on the critical path of
//! logging into your own house, and would mean every household's sign-ins pass
//! through infrastructure we run. ADR-0016 keeps the trust one-way on purpose,
//! and this does not add an edge back.
//!
//! Federated login is therefore **optional and absent until configured**. Local
//! password login is the floor and is never removable.
//!
//! # userinfo, not `id_token`
//!
//! The authorization code is exchanged for an access token, and the access token
//! is spent on the provider's **userinfo** endpoint. Verifying an `id_token`
//! instead would mean a JOSE stack: JWKS fetching, key-rotation handling, and
//! algorithm-confusion foot-guns. This repository deliberately avoids one (push
//! VAPID is hand-signed with `p256`). Hitting userinfo over TLS gets the same
//! answer from the same authority with none of that surface.
//!
//! # The subject is the provider's immutable id
//!
//! Google's `sub`, `GitHub`'s **numeric** `id` — never the email, and never the
//! `GitHub` login. Emails get reassigned and `GitHub` logins can be renamed and
//! reused, so either would let a later holder of a name inherit an account.
//! `GitHub`'s primary email may also be private, so email cannot be assumed
//! present at all.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::AppError;
use wardnetd_data::repository::user_credential::CredentialKind;

/// Hard ceiling on every provider call.
///
/// A dead or throttling WAN must not pin a worker: sign-in is a foreground
/// request, and a provider that has not answered in ten seconds is not going to
/// make this login succeed.
pub const PROVIDER_TIMEOUT: Duration = Duration::from_secs(10);

/// Which federated provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OauthProvider {
    Google,
    Github,
}

impl OauthProvider {
    /// The URL-path and config-key token for this provider.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Github => "github",
        }
    }

    /// Parse a provider from a URL path segment. `None` for anything else —
    /// an unrecognised provider must not fall through to a configured one.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "google" => Some(Self::Google),
            "github" => Some(Self::Github),
            _ => None,
        }
    }

    /// The credential kind this provider's links are stored under.
    #[must_use]
    pub const fn credential_kind(self) -> CredentialKind {
        match self {
            Self::Google => CredentialKind::Google,
            Self::Github => CredentialKind::Github,
        }
    }

    /// `system_config` key holding the household's client id.
    #[must_use]
    pub fn client_id_key(self) -> String {
        format!("identity_oauth_{}_client_id", self.as_str())
    }

    /// `system_config` key holding the enabled flag.
    #[must_use]
    pub fn enabled_key(self) -> String {
        format!("identity_oauth_{}_enabled", self.as_str())
    }

    /// `SecretStore` path holding the client secret.
    ///
    /// Never returned by any API — reads report `configured: true|false`.
    #[must_use]
    pub fn secret_path(self) -> String {
        format!("identity/oauth/{}/client_secret", self.as_str())
    }

    /// The redirect URI the admin must register with the provider.
    #[must_use]
    pub fn redirect_uri(self, canonical_fqdn: &str) -> String {
        format!(
            "https://{canonical_fqdn}/api/auth/oauth/{}/callback",
            self.as_str()
        )
    }
}

/// Provider endpoints.
///
/// Struct fields with production defaults rather than `const`s, so a test can
/// point them at a `wiremock` server. Making these constants would mean the
/// only way to test the exchange was to talk to Google.
#[derive(Debug, Clone)]
pub struct ProviderEndpoints {
    /// Where the browser is sent to consent.
    pub authorize_url: String,
    /// Where the authorization code is exchanged for an access token.
    pub token_url: String,
    /// Where the access token is spent to learn who signed in.
    pub userinfo_url: String,
    /// Scopes requested at the authorize step.
    pub scopes: String,
}

impl ProviderEndpoints {
    /// Production endpoints for `provider`.
    #[must_use]
    pub fn production(provider: OauthProvider) -> Self {
        match provider {
            OauthProvider::Google => Self {
                authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".to_owned(),
                token_url: "https://oauth2.googleapis.com/token".to_owned(),
                userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo".to_owned(),
                // `openid email` only. Wardnet wants an identifier and a label,
                // not a person's contact list.
                scopes: "openid email".to_owned(),
            },
            OauthProvider::Github => Self {
                authorize_url: "https://github.com/login/oauth/authorize".to_owned(),
                token_url: "https://github.com/login/oauth/access_token".to_owned(),
                userinfo_url: "https://api.github.com/user".to_owned(),
                // `read:user` for the numeric id and login; `user:email`
                // because GitHub omits a private primary email from /user.
                scopes: "read:user user:email".to_owned(),
            },
        }
    }
}

/// The state carried across an OAuth round-trip.
#[derive(Debug, Clone)]
pub struct PendingOauth {
    /// Which provider the ceremony was started for. Checked on callback so a
    /// `state` minted for one provider cannot be redeemed at another.
    pub provider: OauthProvider,
    /// The PKCE verifier whose challenge went out with the authorize request.
    pub pkce_verifier: String,
    /// Who started the ceremony, when it was started by a signed-in user in
    /// order to **link** a provider account.
    ///
    /// `None` for a sign-in ceremony, which by definition has no caller yet.
    ///
    /// Binding the owner is what stops an attacker starting a ceremony with
    /// their own provider account, obtaining `(state, code)`, and then getting a
    /// signed-in admin's browser to redeem it — which would attach the
    /// attacker's Google account to the admin's user and hand them the
    /// household. `finish_passkey_registration` refuses on the same mismatch;
    /// this makes the OAuth link path match.
    pub started_by: Option<uuid::Uuid>,
}

/// Who the provider says signed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderIdentity {
    /// The provider's immutable subject: Google `sub`, `GitHub` numeric id.
    pub subject: String,
    /// Email, when the provider discloses one. Never used as the join key.
    pub email: Option<String>,
    /// Human label for the credential list ("octocat").
    pub label: Option<String>,
}

/// Generate a PKCE verifier and its S256 challenge.
///
/// PKCE is not optional here even though the client secret is held
/// server-side: it binds the authorization code to *this* ceremony, so a code
/// intercepted from the redirect cannot be redeemed by anybody else.
#[must_use]
pub fn new_pkce_pair() -> (String, String) {
    let bytes: [u8; 32] = rand::random();
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// Generate an opaque `state` value.
#[must_use]
pub fn new_state() -> String {
    let bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Google's userinfo response. Only the fields we use.
#[derive(Debug, Deserialize)]
struct GoogleUserinfo {
    sub: String,
    email: Option<String>,
}

/// `GitHub`'s `/user` response. `id` is numeric on the wire.
#[derive(Debug, Deserialize)]
struct GithubUser {
    id: i64,
    login: Option<String>,
    email: Option<String>,
}

/// A provider's token response.
///
/// `GitHub` returns form-encoded by default, which is why the caller sends
/// `Accept: application/json`.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// Everything one authorization-code exchange needs.
///
/// Grouped into a struct rather than passed as seven positional arguments —
/// the same reasoning as `NewPushSubscription`. Four of them are `&str` and
/// two of those are secrets, so a transposed pair would compile silently and
/// fail at the provider.
#[derive(Debug, Clone, Copy)]
pub struct OauthExchange<'a> {
    pub provider: OauthProvider,
    pub endpoints: &'a ProviderEndpoints,
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub redirect_uri: &'a str,
    /// The authorization code the provider handed the browser.
    pub code: &'a str,
    /// The PKCE verifier whose challenge went out with the authorize request.
    pub pkce_verifier: &'a str,
}

/// Talks to a provider. A trait so the service can be tested without a network
/// and so `wiremock` has something to stand in front of.
#[async_trait::async_trait]
pub trait OauthClient: Send + Sync {
    /// Exchange an authorization code for an access token, then resolve it to
    /// the provider's view of the user.
    async fn exchange_and_identify(
        &self,
        exchange: OauthExchange<'_>,
    ) -> Result<ProviderIdentity, AppError>;
}

/// `reqwest`-backed [`OauthClient`].
pub struct ReqwestOauthClient {
    http: reqwest::Client,
}

impl ReqwestOauthClient {
    /// Build a client with the hard [`PROVIDER_TIMEOUT`] applied.
    pub fn new() -> Result<Self, AppError> {
        let http = reqwest::Client::builder()
            .timeout(PROVIDER_TIMEOUT)
            // A `User-Agent` is mandatory for the GitHub API and harmless
            // elsewhere; requests without one are rejected.
            .user_agent(concat!("wardnet/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to build http client: {e}")))?;
        Ok(Self { http })
    }
}

#[async_trait::async_trait]
impl OauthClient for ReqwestOauthClient {
    async fn exchange_and_identify(
        &self,
        exchange: OauthExchange<'_>,
    ) -> Result<ProviderIdentity, AppError> {
        let OauthExchange {
            provider,
            endpoints,
            client_id,
            client_secret,
            redirect_uri,
            code,
            pkce_verifier,
        } = exchange;

        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code_verifier", pkce_verifier),
        ];

        let token: TokenResponse = self
            .http
            .post(&endpoints.token_url)
            // GitHub form-encodes its token response unless asked otherwise.
            .header(reqwest::header::ACCEPT, "application/json")
            .form(&form)
            .send()
            .await
            .map_err(|e| map_provider_error(&e))?
            .error_for_status()
            .map_err(|e| map_provider_error(&e))?
            .json()
            .await
            .map_err(|e| map_provider_error(&e))?;

        let response = self
            .http
            .get(&endpoints.userinfo_url)
            .bearer_auth(&token.access_token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| map_provider_error(&e))?
            .error_for_status()
            .map_err(|e| map_provider_error(&e))?;

        match provider {
            OauthProvider::Google => {
                let info: GoogleUserinfo =
                    response.json().await.map_err(|e| map_provider_error(&e))?;
                Ok(ProviderIdentity {
                    subject: info.sub,
                    label: info.email.clone(),
                    email: info.email,
                })
            }
            OauthProvider::Github => {
                let user: GithubUser = response.json().await.map_err(|e| map_provider_error(&e))?;
                Ok(ProviderIdentity {
                    // The numeric id, stringified — never the login, which is
                    // renameable and reusable.
                    subject: user.id.to_string(),
                    email: user.email,
                    label: user.login,
                })
            }
        }
    }
}

/// Map a provider transport or status failure to a `502`.
///
/// Deliberately **not** `Unauthorized`: the caller's credentials were not
/// rejected, the provider was unreachable or broke. Reporting 401 would send a
/// household hunting for a password problem during somebody else's outage.
fn map_provider_error(err: &reqwest::Error) -> AppError {
    if err.is_timeout() {
        return AppError::UpstreamUnavailable("the identity provider timed out".to_owned());
    }
    AppError::UpstreamUnavailable(format!("the identity provider failed: {err}"))
}

/// What `GET /api/auth/methods` reports for one provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderStatus {
    pub provider: OauthProvider,
    /// The admin turned it on **and** it is fully configured.
    pub enabled: bool,
    /// A client secret is present. The secret itself is never returned.
    pub configured: bool,
    /// The redirect URI to register with the provider, when a canonical FQDN
    /// exists. `None` means federated login cannot work yet, and the UI should
    /// say why rather than showing a broken button.
    pub redirect_uri: Option<String>,
}

/// Shared config accessor used by the user service's OAuth paths.
pub struct OauthConfig {
    pub system_config: Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
    pub secrets: Arc<dyn wardnetd_data::secret_store::SecretStore>,
}

impl OauthConfig {
    /// The household's canonical public hostname, if it has one.
    ///
    /// Read from the same `system_config` keys the DDNS subsystem writes, rather
    /// than a second copy that could drift. `None` on a box with no public
    /// hostname — where a provider could not reach a redirect URI anyway, so
    /// federated login is correctly unavailable.
    pub async fn canonical_fqdn(&self) -> Result<Option<String>, AppError> {
        use crate::ddns::{
            KEY_DOMAIN, KEY_PROVIDER, KEY_SUBDOMAIN, PROVIDER_CLOUDFLARE, PROVIDER_WARDNET,
        };

        let provider = self
            .system_config
            .get(KEY_PROVIDER)
            .await
            .map_err(AppError::Internal)?;

        let key = match provider.as_deref() {
            Some(PROVIDER_WARDNET) => KEY_SUBDOMAIN,
            Some(PROVIDER_CLOUDFLARE) => KEY_DOMAIN,
            _ => return Ok(None),
        };

        self.system_config
            .get(key)
            .await
            .map_err(AppError::Internal)
    }

    /// The client id an admin configured, if any.
    pub async fn client_id(&self, provider: OauthProvider) -> Result<Option<String>, AppError> {
        self.system_config
            .get(&provider.client_id_key())
            .await
            .map_err(AppError::Internal)
    }

    /// The client secret. Never leaves the service layer.
    pub async fn client_secret(&self, provider: OauthProvider) -> Result<Option<String>, AppError> {
        let raw = self
            .secrets
            .get(&provider.secret_path())
            .await
            .map_err(AppError::Internal)?;
        match raw {
            Some(bytes) => Ok(Some(String::from_utf8(bytes).map_err(|_| {
                AppError::Internal(anyhow::anyhow!("stored client secret is not valid UTF-8"))
            })?)),
            None => Ok(None),
        }
    }

    /// Whether the admin has switched this provider on.
    pub async fn is_enabled(&self, provider: OauthProvider) -> Result<bool, AppError> {
        Ok(self
            .system_config
            .get(&provider.enabled_key())
            .await
            .map_err(AppError::Internal)?
            .as_deref()
            == Some("true"))
    }

    /// The full status a UI needs to decide whether to render a button.
    ///
    /// `enabled` requires *all three* of the admin's flag, a client id, and a
    /// stored secret. Reporting enabled on a half-configured provider would
    /// render a button that always fails.
    pub async fn status(&self, provider: OauthProvider) -> Result<ProviderStatus, AppError> {
        let fqdn = self.canonical_fqdn().await?;
        let has_id = self.client_id(provider).await?.is_some();
        let configured = self.client_secret(provider).await?.is_some();
        let flag = self.is_enabled(provider).await?;

        Ok(ProviderStatus {
            provider,
            enabled: flag && has_id && configured && fqdn.is_some(),
            configured,
            redirect_uri: fqdn.as_deref().map(|f| provider.redirect_uri(f)),
        })
    }
}
