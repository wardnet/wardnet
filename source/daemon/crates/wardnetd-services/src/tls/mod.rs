//! Daemon-owned TLS termination — obtain, store, and renew the public cert.
//!
//! ## Shape
//!
//! ```text
//! TlsRenewalRunner  ──(admin auth ctx)──▶  TlsService  ──▶  acme (instant-acme)
//!  (12h tick)                              (auth-gated)   │     └─▶ DdnsService.set_acme_challenge
//!                                                         └─▶ CertActivator.activate (hot-swap :443)
//! ```
//!
//! The [`TlsRenewalRunner`](runner::TlsRenewalRunner) holds only
//! `Arc<dyn TlsService>` and calls it under an admin context — it never touches
//! a provider, repository, or the ACME client directly (see
//! `.agents/architecture.md`). [`TlsService`] is the auth-and-persistence
//! chokepoint: every method opens with [`auth_context::require_admin`].
//!
//! TLS-01 / DNS-01 challenge TXT records are published through
//! [`DdnsService::set_acme_challenge`](crate::ddns::DdnsService::set_acme_challenge)
//! — the same provider abstraction that keeps the A record current — rather than
//! a second provider-construction path.
//!
//! ## Storage
//!
//! Non-secret state lives in `system_config` (`tls_cert_domain`,
//! `tls_cert_not_after`, `acme_directory_url`). The ACME account credentials and
//! the issued chain + leaf private key live in the on-Pi [`SecretStore`] under
//! `tls/…`. The private key is generated locally during issuance and **never
//! leaves the LAN**; whatever at-rest representation the configured
//! `SecretStore` backend chooses is its concern, not this module's.
//!
//! ## Live-cert swap
//!
//! Serving lives in `wardnetd` (it owns `axum-server`). The
//! [`CertActivator`] trait is the seam: `wardnetd` provides an impl that holds
//! the `:443` `RustlsConfig` + a `provisioned` flag; `activate()` reloads the
//! cert in-memory and flips the flag so the 503 "not provisioned" guard lifts.

pub mod acme;
pub mod runner;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use wardnetd_data::repository::SystemConfigRepository;

use crate::auth_context;
use crate::ddns::DdnsService;
use crate::error::AppError;
use crate::secret_store::SecretStore;

// ── system_config keys (non-secret) ─────────────────────────────────────────────
pub(crate) const KEY_CERT_DOMAIN: &str = "tls_cert_domain";
pub(crate) const KEY_CERT_NOT_AFTER: &str = "tls_cert_not_after";
pub(crate) const KEY_ACME_DIRECTORY_URL: &str = "acme_directory_url";

// ── secret-store paths ───────────────────────────────────────────────────────────
pub(crate) const SECRET_ACME_ACCOUNT: &str = "tls/acme/account";
pub(crate) const SECRET_CERT_CHAIN: &str = "tls/cert/chain_pem";
pub(crate) const SECRET_CERT_KEY: &str = "tls/cert/key_pem";

/// Default ACME directory — Let's Encrypt production. Overridable via the
/// `acme_directory_url` config key (LE staging or Pebble for tests / manual runs).
const DEFAULT_ACME_DIRECTORY_URL: &str = "https://acme-v02.api.letsencrypt.org/directory";

/// Renew once the stored cert is within this many days of `not_after`.
const RENEWAL_WINDOW_DAYS: i64 = 30;

/// Whether a cert expiring at `not_after` should be renewed as of `now` — i.e.
/// it is within (or past) the [`RENEWAL_WINDOW_DAYS`] window. Pure so the
/// renewal threshold is unit-testable without an ACME server.
pub(crate) fn within_renewal_window(not_after: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    not_after - now <= Duration::days(RENEWAL_WINDOW_DAYS)
}

/// Read the stored `(chain_pem, key_pem)` from the secret store, if a
/// certificate has been issued. Used by `wardnetd` to seed the `:443`
/// `RustlsConfig` at boot. Returns `None` unless **both** halves are present —
/// keeping cert access behind the `SecretStore` abstraction (no direct
/// filesystem reads).
pub async fn load_stored_cert(
    secrets: &dyn SecretStore,
) -> anyhow::Result<Option<(Vec<u8>, Vec<u8>)>> {
    let (Some(chain), Some(key)) = (
        secrets.get(SECRET_CERT_CHAIN).await?,
        secrets.get(SECRET_CERT_KEY).await?,
    ) else {
        return Ok(None);
    };
    Ok(Some((chain, key)))
}

/// Swaps the live `:443` certificate. Implemented in `wardnetd` (which owns the
/// `axum-server` listener); kept as a trait here so this crate has no dependency
/// on the serving stack and the renewal flow stays unit-testable.
#[async_trait]
pub trait CertActivator: Send + Sync {
    /// Hot-swap the live `:443` cert to (`chain_pem`, `key_pem`) and mark TLS
    /// provisioned. Idempotent on the flag: renewal re-activates with the flag
    /// already `true`.
    async fn activate(&self, chain_pem: Vec<u8>, key_pem: Vec<u8>) -> anyhow::Result<()>;
}

/// Current TLS state, surfaced to Settings / status views (C10) and logged by
/// the renewal runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsStatus {
    /// No active FQDN (DDNS unconfigured) — issuance is inert.
    NotConfigured,
    /// An FQDN is configured but no certificate has been issued yet.
    Pending { domain: String },
    /// A valid certificate is stored and not within the renewal window.
    UpToDate {
        domain: String,
        not_after: DateTime<Utc>,
    },
    /// A certificate is stored for the active domain but is within the renewal
    /// window (or already expired) — renewal is due. Surfaced by [`TlsService::status`];
    /// [`TlsService::ensure_certificate`] acts on this state rather than returning it.
    NeedsRenewal {
        domain: String,
        not_after: DateTime<Utc>,
    },
    /// A certificate was issued or renewed during this call.
    Issued {
        domain: String,
        not_after: DateTime<Utc>,
    },
}

/// Auth-gated TLS operations. Every method requires an admin context.
#[async_trait]
pub trait TlsService: Send + Sync {
    /// Issue-if-missing or renew-if-expiring (within [`RENEWAL_WINDOW_DAYS`] of
    /// `not_after`) the public certificate, then hot-swap it onto `:443`. Inert
    /// (`Ok(NotConfigured)`) when no FQDN is active. Idempotent — safe to call
    /// from the renewal runner, the wizard (C9), and Settings (C10).
    async fn ensure_certificate(&self) -> Result<TlsStatus, AppError>;

    /// Read the current TLS status without touching the ACME server.
    async fn status(&self) -> Result<TlsStatus, AppError>;
}

/// The concrete [`TlsService`].
pub struct TlsServiceImpl {
    config: Arc<dyn SystemConfigRepository>,
    secrets: Arc<dyn SecretStore>,
    ddns: Arc<dyn DdnsService>,
    activator: Arc<dyn CertActivator>,
}

impl TlsServiceImpl {
    /// Build the service from its repository, secret store, DDNS service, and
    /// the `wardnetd`-provided certificate activator.
    #[must_use]
    pub fn new(
        config: Arc<dyn SystemConfigRepository>,
        secrets: Arc<dyn SecretStore>,
        ddns: Arc<dyn DdnsService>,
        activator: Arc<dyn CertActivator>,
    ) -> Self {
        Self {
            config,
            secrets,
            ddns,
            activator,
        }
    }

    async fn get_cfg(&self, key: &str) -> Result<Option<String>, AppError> {
        self.config.get(key).await.map_err(AppError::Internal)
    }

    async fn set_cfg(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.config
            .set(key, value)
            .await
            .map_err(AppError::Internal)
    }

    /// The configured ACME directory URL, or the Let's Encrypt prod default.
    ///
    /// Rejects a non-`https://` value (defence-in-depth: the key is admin-gated
    /// but a bad value would point issuance at an arbitrary server) and warns
    /// when it deviates from the LE-prod default so staging/Pebble overrides are
    /// visible in the logs.
    async fn directory_url(&self) -> Result<String, AppError> {
        let url = self
            .get_cfg(KEY_ACME_DIRECTORY_URL)
            .await?
            .unwrap_or_else(|| DEFAULT_ACME_DIRECTORY_URL.to_owned());
        if !url.starts_with("https://") {
            return Err(AppError::Conflict(format!(
                "acme_directory_url must be an https:// URL, got: {url}"
            )));
        }
        if url != DEFAULT_ACME_DIRECTORY_URL {
            tracing::warn!(%url, "using non-default ACME directory URL: url={url}");
        }
        Ok(url)
    }

    /// The stored cert's `not_after`, if a previous issuance recorded one.
    async fn stored_not_after(&self) -> Result<Option<DateTime<Utc>>, AppError> {
        let Some(raw) = self.get_cfg(KEY_CERT_NOT_AFTER).await? else {
            return Ok(None);
        };
        let parsed = DateTime::parse_from_rfc3339(&raw)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid stored not_after: {e}")))?
            .with_timezone(&Utc);
        Ok(Some(parsed))
    }

    /// Whether a stored cert for `domain` is fresh (outside the renewal window).
    async fn stored_cert_is_fresh(&self, domain: &str) -> Result<Option<DateTime<Utc>>, AppError> {
        let stored_domain = self.get_cfg(KEY_CERT_DOMAIN).await?;
        if stored_domain.as_deref() != Some(domain) {
            return Ok(None); // domain changed → must reissue
        }
        let Some(not_after) = self.stored_not_after().await? else {
            return Ok(None);
        };
        if within_renewal_window(not_after, Utc::now()) {
            Ok(None)
        } else {
            Ok(Some(not_after))
        }
    }
}

#[async_trait]
impl TlsService for TlsServiceImpl {
    async fn ensure_certificate(&self) -> Result<TlsStatus, AppError> {
        auth_context::require_admin()?;

        // Inert when DDNS (and therefore the public FQDN) is unconfigured —
        // mirrors DdnsUpdateRunner's inert-until-config behaviour.
        let Some(domain) = self.ddns.status().await?.fqdn else {
            return Ok(TlsStatus::NotConfigured);
        };

        if let Some(not_after) = self.stored_cert_is_fresh(&domain).await? {
            return Ok(TlsStatus::UpToDate { domain, not_after });
        }

        let directory_url = self.directory_url().await?;
        let issued = acme::issue(
            self.ddns.as_ref(),
            self.secrets.as_ref(),
            &directory_url,
            &domain,
        )
        .await
        .map_err(|e| AppError::UpstreamUnavailable(format!("ACME issuance failed: {e}")))?;

        let not_after = acme::parse_not_after(issued.chain_pem.as_bytes())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;

        // Secrets first, so a partial failure never records a `not_after` for a
        // cert we couldn't persist.
        self.secrets
            .put(SECRET_CERT_CHAIN, issued.chain_pem.as_bytes())
            .await
            .map_err(AppError::Internal)?;
        self.secrets
            .put(SECRET_CERT_KEY, issued.key_pem.as_bytes())
            .await
            .map_err(AppError::Internal)?;
        self.set_cfg(KEY_CERT_DOMAIN, &domain).await?;
        self.set_cfg(KEY_CERT_NOT_AFTER, &not_after.to_rfc3339())
            .await?;

        self.activator
            .activate(issued.chain_pem.into_bytes(), issued.key_pem.into_bytes())
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            %domain,
            %not_after,
            "issued/renewed TLS certificate for {domain}, valid until {not_after}"
        );
        Ok(TlsStatus::Issued { domain, not_after })
    }

    async fn status(&self) -> Result<TlsStatus, AppError> {
        auth_context::require_admin()?;
        let Some(domain) = self.ddns.status().await?.fqdn else {
            return Ok(TlsStatus::NotConfigured);
        };
        // A stored cert only counts for the *current* domain — if the FQDN
        // changed, the old cert is unusable and a fresh issuance is pending.
        if self.get_cfg(KEY_CERT_DOMAIN).await?.as_deref() != Some(domain.as_str()) {
            return Ok(TlsStatus::Pending { domain });
        }
        match self.stored_not_after().await? {
            Some(not_after) if within_renewal_window(not_after, Utc::now()) => {
                Ok(TlsStatus::NeedsRenewal { domain, not_after })
            }
            Some(not_after) => Ok(TlsStatus::UpToDate { domain, not_after }),
            None => Ok(TlsStatus::Pending { domain }),
        }
    }
}
