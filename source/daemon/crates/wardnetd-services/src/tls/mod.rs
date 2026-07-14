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
//! [`CertActivator`] trait is the seam: `wardnetd` provides an impl (the
//! serving-control component) that holds the `:443` `RustlsConfig` + the
//! currently-served domain; `activate()` reloads the cert in-memory and records
//! the domain, which lifts the 503 "not provisioned" guard and becomes the
//! canonical short-name redirect target. The serving component exposes that
//! state through methods (not a shared flag) so the unauthenticated `:80`/`:443`
//! listeners never read raw shared memory or call an admin-gated service.

pub mod acme;
pub mod runner;

#[cfg(test)]
mod tests;

use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use wardnet_common::api::{TlsProvisioningPhase, TlsStatusResponse, UpsertRecordRequest};
use wardnet_common::dns::{DnsRecordSource, DnsRecordType};
use wardnetd_data::repository::SystemConfigRepository;

use crate::auth_context;
use crate::ddns::DdnsService;
use crate::dns_local::DnsLocalService;
use crate::error::AppError;
use crate::secret_store::SecretStore;

/// TTL (seconds) for the daemon-maintained split-horizon / LAN system records.
/// Short enough that a canonical-FQDN change propagates to LAN clients within a
/// bounded window, without being so low it hammers the resolver.
const SYSTEM_RECORD_TTL_SECS: u32 = 300;

// ── system_config keys (non-secret) ─────────────────────────────────────────────
pub(crate) const KEY_CERT_DOMAIN: &str = "tls_cert_domain";
pub(crate) const KEY_CERT_NOT_AFTER: &str = "tls_cert_not_after";
pub(crate) const KEY_ACME_DIRECTORY_URL: &str = "acme_directory_url";
/// Coarse provisioning phase (`idle`/`issuing`/`issued`/`failed`), surfaced to
/// the wizard + dashboard so they can show live progress without instrumenting
/// the opaque ACME round-trip.
pub(crate) const KEY_TLS_PHASE: &str = "tls_provisioning_phase";
/// Human-readable error from the last failed issuance (set with `failed`).
pub(crate) const KEY_TLS_LAST_ERROR: &str = "tls_provisioning_error";

/// Stable lowercase identifier persisted for [`TlsProvisioningPhase`]. Mirrors
/// the serde `snake_case` representation, kept explicit so persistence is
/// decoupled from the wire format.
fn phase_storage_str(phase: TlsProvisioningPhase) -> &'static str {
    match phase {
        TlsProvisioningPhase::Idle => "idle",
        TlsProvisioningPhase::Issuing => "issuing",
        TlsProvisioningPhase::Issued => "issued",
        TlsProvisioningPhase::Failed => "failed",
    }
}

/// Parse a persisted phase; unknown/missing values are treated as `Idle`.
fn phase_from_storage(s: &str) -> TlsProvisioningPhase {
    match s {
        "issuing" => TlsProvisioningPhase::Issuing,
        "issued" => TlsProvisioningPhase::Issued,
        "failed" => TlsProvisioningPhase::Failed,
        _ => TlsProvisioningPhase::Idle,
    }
}

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

/// The domain recorded for the stored certificate (`tls_cert_domain`), if any.
/// Used by `wardnetd` to seed the serving identity at boot so an already-issued
/// cert lifts the 503 gate (and becomes the redirect target) without waiting for
/// the first renewal tick. Kept here so `wardnetd` reads the config key through a
/// service helper rather than knowing the key name.
pub async fn load_stored_cert_domain(
    config: &dyn SystemConfigRepository,
) -> anyhow::Result<Option<String>> {
    config.get(KEY_CERT_DOMAIN).await
}

/// Swaps the live `:443` certificate. Implemented in `wardnetd` (which owns the
/// `axum-server` listener); kept as a trait here so this crate has no dependency
/// on the serving stack and the renewal flow stays unit-testable.
#[async_trait]
pub trait CertActivator: Send + Sync {
    /// Hot-swap the live `:443` cert to (`chain_pem`, `key_pem`) and record
    /// `fqdn` as the domain now being served (which lifts the "not provisioned"
    /// gate and becomes the canonical redirect target). Idempotent: renewal
    /// re-activates with the same domain already live.
    async fn activate(
        &self,
        chain_pem: Vec<u8>,
        key_pem: Vec<u8>,
        fqdn: String,
    ) -> anyhow::Result<()>;

    /// Revert `:443` to the unprovisioned state: swap a fresh placeholder cert
    /// back in and clear the served domain, so the 503 gate closes and the
    /// short-name redirect target goes inert. The inverse of [`Self::activate`],
    /// called by [`TlsService::teardown`] when remote access is removed.
    /// Idempotent: deactivating when already unprovisioned is a no-op swap.
    async fn deactivate(&self) -> anyhow::Result<()>;
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

    /// Mark provisioning as started (phase → `Issuing`, error cleared). Called
    /// synchronously by the wizard's register endpoint *before* it spawns the
    /// background issuance task, so a status poll immediately after register
    /// already reflects `issuing` rather than racing the spawned task.
    async fn mark_provisioning_started(&self) -> Result<(), AppError>;

    /// Coarse provisioning status (phase + domain + expiry + error) for the
    /// setup wizard and dashboard indicator. Does not touch the ACME server.
    async fn provisioning_status(&self) -> Result<TlsStatusResponse, AppError>;

    /// Tear down TLS provisioning: revert `:443` to the placeholder cert (closing
    /// the 503 gate), prune the split-horizon record for the torn-down domain,
    /// and clear the stored certificate + provisioning state. The ACME **account**
    /// credential is deliberately kept, so a later re-enable reuses it rather than
    /// registering a fresh account (avoiding CA rate-limit churn). Idempotent and
    /// best-effort on the serving + DNS side — those failures are logged, never
    /// fatal. Paired with [`DdnsService::teardown`](crate::ddns::DdnsService::teardown)
    /// by the disable handler.
    ///
    /// **Precondition:** an admin [`auth_context`] must be active — the nested
    /// `DnsLocalService` calls open with `require_admin()?` (see
    /// [`TlsServiceImpl::reconcile_split_horizon`]).
    async fn teardown(&self) -> Result<(), AppError>;
}

/// The concrete [`TlsService`].
pub struct TlsServiceImpl {
    config: Arc<dyn SystemConfigRepository>,
    secrets: Arc<dyn SecretStore>,
    ddns: Arc<dyn DdnsService>,
    activator: Arc<dyn CertActivator>,
    /// Used to maintain the split-horizon `<fqdn> → lan_ip` system record.
    /// Reached through the auth-gated service (never a repository); calls inherit
    /// the renewal runner's admin `auth_context`.
    dns_local: Arc<dyn DnsLocalService>,
    /// The Pi's LAN-interface IPv4 — the value of the split-horizon record.
    lan_ip: Ipv4Addr,
    /// Single-flight guard for issuance: the renewal runner and the
    /// register-time provisioning task can both call
    /// [`TlsService::ensure_certificate`] concurrently, and two interleaved
    /// ACME orders share the `_acme-challenge.<domain>` name — the first to
    /// finish clears the TXT records out from under the other's still-pending
    /// validation. Serialising is safe because the call is idempotent: the
    /// second caller finds the fresh certificate and no-ops.
    issuance: tokio::sync::Mutex<()>,
}

impl TlsServiceImpl {
    /// Build the service from its repository, secret store, DDNS service, the
    /// `wardnetd`-provided certificate activator, the local-DNS service (for the
    /// split-horizon record), and the Pi's LAN IP.
    #[must_use]
    pub fn new(
        config: Arc<dyn SystemConfigRepository>,
        secrets: Arc<dyn SecretStore>,
        ddns: Arc<dyn DdnsService>,
        activator: Arc<dyn CertActivator>,
        dns_local: Arc<dyn DnsLocalService>,
        lan_ip: Ipv4Addr,
    ) -> Self {
        Self {
            config,
            secrets,
            ddns,
            activator,
            dns_local,
            lan_ip,
            issuance: tokio::sync::Mutex::new(()),
        }
    }

    /// Reconcile the split-horizon `<domain> → lan_ip` system A record so LAN
    /// clients resolving the canonical FQDN reach the Pi directly (with the valid
    /// cert). Idempotent; called on every successful `ensure_certificate` so the
    /// record self-heals. When `previous_domain` names a different earlier FQDN,
    /// its stale system record is removed.
    ///
    /// **Non-fatal:** the cert is already live by the time this runs, so a
    /// local-DNS failure must never surface as an issuance error — it is logged
    /// and swallowed (mirrors `DhcpLanRunner`).
    ///
    /// **Precondition:** must be called from within an admin [`auth_context`] —
    /// the `DnsLocalService` methods it calls open with `require_admin()?`. Its
    /// only caller, `ensure_certificate`, runs under the renewal runner's admin
    /// context, which these nested calls inherit (a service inheriting its
    /// caller's verified context — *not* a runner force-establishing one).
    async fn reconcile_split_horizon(&self, domain: &str, previous_domain: Option<&str>) {
        let lan_ip = self.lan_ip;
        let req = UpsertRecordRequest {
            zone_id: None,
            domain: domain.to_owned(),
            record_type: DnsRecordType::A,
            value: lan_ip.to_string(),
            ttl: SYSTEM_RECORD_TTL_SECS,
            enabled: true,
            source: DnsRecordSource::System,
        };
        match self.dns_local.upsert_record(req).await {
            Ok(_) => tracing::info!(
                %domain, %lan_ip,
                "reconciled split-horizon record {domain} -> {lan_ip}",
            ),
            Err(e) => tracing::warn!(
                error = %e, %domain,
                "failed to reconcile split-horizon record for {domain}; LAN clients will hairpin via WAN"
            ),
        }

        if let Some(prev) = previous_domain
            && prev != domain
        {
            self.delete_system_record(prev).await;
        }
    }

    /// Remove the daemon-seeded `System` A record for `domain` (used when the
    /// canonical FQDN changes). Non-fatal — a leftover record is harmless. The
    /// service exposes no delete-by-name, so find-then-delete via the listing.
    ///
    /// **Precondition:** an admin [`auth_context`] must be active (see
    /// [`Self::reconcile_split_horizon`]).
    async fn delete_system_record(&self, domain: &str) {
        let records = match self.dns_local.list_records().await {
            Ok(resp) => resp.records,
            Err(e) => {
                tracing::warn!(error = %e, %domain, "failed to list records to prune stale system record for {domain}");
                return;
            }
        };
        for rec in records.into_iter().filter(|r| {
            r.domain == domain
                && r.record_type == DnsRecordType::A
                && r.source == DnsRecordSource::System
        }) {
            if let Err(e) = self.dns_local.delete_record(rec.id).await {
                tracing::warn!(error = %e, %domain, "failed to delete stale system record for {domain}");
            } else {
                tracing::info!(%domain, "pruned stale split-horizon record for {domain}");
            }
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

    /// Persist the provisioning phase, recording `error` only for `Failed` and
    /// clearing any stale error otherwise.
    async fn set_phase(
        &self,
        phase: TlsProvisioningPhase,
        error: Option<&str>,
    ) -> Result<(), AppError> {
        self.set_cfg(KEY_TLS_PHASE, phase_storage_str(phase))
            .await?;
        self.set_cfg(KEY_TLS_LAST_ERROR, error.unwrap_or("")).await
    }

    /// Record a failed issuance — best-effort, so it never masks the original
    /// error it is annotating.
    async fn record_failure(&self, msg: &str) {
        if let Err(e) = self
            .set_phase(TlsProvisioningPhase::Failed, Some(msg))
            .await
        {
            tracing::warn!(error = %e, "failed to persist TLS provisioning failure marker");
        }
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

    /// Run a full ACME issuance for `domain`: obtain the cert, persist it, swap
    /// it onto `:443`, reconcile the split-horizon record (pruning
    /// `previous_domain` on a change), and record the new domain + expiry.
    /// Returns the issued cert's `not_after`. Phase bookkeeping is the caller's
    /// (`ensure_certificate`) responsibility.
    async fn issue_and_activate(
        &self,
        domain: &str,
        previous_domain: Option<&str>,
    ) -> Result<DateTime<Utc>, AppError> {
        let directory_url = self.directory_url().await?;
        let issued = acme::issue(
            self.ddns.as_ref(),
            self.secrets.as_ref(),
            &directory_url,
            domain,
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

        // Make the cert live, then reconcile the split-horizon record (deleting
        // the prior domain's record on a change), then record the new domain as
        // the truth — so the stale-delete above still sees the *old* domain.
        self.activator
            .activate(
                issued.chain_pem.into_bytes(),
                issued.key_pem.into_bytes(),
                domain.to_owned(),
            )
            .await
            .map_err(AppError::Internal)?;

        self.reconcile_split_horizon(domain, previous_domain).await;

        // `activate` is irreversible: the cert + served domain are already live in
        // `ServingControl`. If either `set_cfg` below fails (transient DB error),
        // `tls_cert_domain` in `system_config` lags the live identity, so
        // `status()` may briefly report `Pending`/the old domain. This is benign
        // and self-correcting on the next renewal tick — serving is unaffected.
        self.set_cfg(KEY_CERT_DOMAIN, domain).await?;
        self.set_cfg(KEY_CERT_NOT_AFTER, &not_after.to_rfc3339())
            .await?;

        tracing::info!(
            %domain,
            %not_after,
            "issued/renewed TLS certificate for {domain}, valid until {not_after}"
        );
        Ok(not_after)
    }
}

#[async_trait]
impl TlsService for TlsServiceImpl {
    async fn ensure_certificate(&self) -> Result<TlsStatus, AppError> {
        auth_context::require_admin()?;
        // Single-flight: see the `issuance` field. Waiting (rather than
        // try-locking away) is correct — the loser proceeds after the winner
        // and no-ops against the now-fresh certificate.
        let _issuance = self.issuance.lock().await;

        // Inert when DDNS (and therefore the public FQDN) is unconfigured —
        // mirrors DdnsUpdateRunner's inert-until-config behaviour.
        let Some(domain) = self.ddns.status().await?.fqdn else {
            return Ok(TlsStatus::NotConfigured);
        };

        // The domain a prior issuance recorded (for stale split-horizon cleanup
        // on a domain change) — captured before any `set_cfg` overwrites it.
        let previous_domain = self.get_cfg(KEY_CERT_DOMAIN).await?;

        if let Some(not_after) = self.stored_cert_is_fresh(&domain).await? {
            // Steady state: the cert is already live (seeded at boot or activated
            // earlier). Still reconcile the split-horizon record every tick so it
            // self-heals if it was ever lost. The domain is unchanged here (a fresh
            // stored cert *is* for `domain`), so there is nothing stale to prune —
            // pass `None` rather than `previous_domain` (which equals `domain`).
            self.reconcile_split_horizon(&domain, None).await;
            self.set_phase(TlsProvisioningPhase::Issued, None).await?;
            return Ok(TlsStatus::UpToDate { domain, not_after });
        }

        // Past the steady-state short-circuit: a real issuance/renewal is about
        // to run. Mark `Issuing` (idempotent with the wizard's pre-spawn mark)
        // and record any failure so the dashboard can surface it, while still
        // propagating the original error to the caller/runner.
        self.set_phase(TlsProvisioningPhase::Issuing, None).await?;
        match self
            .issue_and_activate(&domain, previous_domain.as_deref())
            .await
        {
            Ok(not_after) => {
                self.set_phase(TlsProvisioningPhase::Issued, None).await?;
                Ok(TlsStatus::Issued { domain, not_after })
            }
            Err(e) => {
                self.record_failure(&e.to_string()).await;
                Err(e)
            }
        }
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

    async fn mark_provisioning_started(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.set_phase(TlsProvisioningPhase::Issuing, None).await
    }

    async fn provisioning_status(&self) -> Result<TlsStatusResponse, AppError> {
        auth_context::require_admin()?;

        let stored_phase = self
            .get_cfg(KEY_TLS_PHASE)
            .await?
            .as_deref()
            .map_or(TlsProvisioningPhase::Idle, phase_from_storage);

        // Prefer the cert's own domain; fall back to the active FQDN so the UI
        // can name the target even before the first cert lands.
        let cert_domain = self.get_cfg(KEY_CERT_DOMAIN).await?;
        let not_after = self.stored_not_after().await?;
        let domain = match cert_domain.clone() {
            Some(d) => Some(d),
            None => self.ddns.status().await?.fqdn,
        };

        // A stored, current-domain cert always reads as `Issued`, even if the
        // marker was never written (e.g. seeded at boot) — the live cert is the
        // source of truth. A persisted `Failed` is preserved so the UI can show
        // the error; otherwise fall through to the stored marker.
        let has_cert = cert_domain.is_some() && not_after.is_some();
        let phase = match stored_phase {
            TlsProvisioningPhase::Failed => TlsProvisioningPhase::Failed,
            _ if has_cert => TlsProvisioningPhase::Issued,
            other => other,
        };

        let error = if phase == TlsProvisioningPhase::Failed {
            self.get_cfg(KEY_TLS_LAST_ERROR)
                .await?
                .filter(|s| !s.is_empty())
        } else {
            None
        };

        Ok(TlsStatusResponse {
            phase,
            domain,
            not_after: not_after.map(|t| t.to_rfc3339()),
            error,
        })
    }

    async fn teardown(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        // Serialise against an in-flight issuance (same lock as
        // `ensure_certificate`): without it, a teardown can win the race
        // partway and the still-running order then re-persists the cert,
        // re-activates `:443`, and re-creates the split-horizon record for a
        // domain the box no longer owns. Waiting the order out (the 15s
        // propagation sleep makes the window generous) and THEN tearing down
        // leaves the intended end state.
        let _issuance = self.issuance.lock().await;

        // Revert :443 to the placeholder cert (503 gate re-engages). Best-effort:
        // a failure leaves the old cert live until the next restart, which is
        // harmless once the FQDN no longer resolves to us.
        if let Err(e) = self.activator.deactivate().await {
            tracing::warn!(error = %e, "failed to revert :443 to placeholder on TLS teardown: {e}");
        }

        // Prune the split-horizon record for the torn-down domain. Read from our
        // own `tls_cert_domain` (not DDNS, which the caller may already have
        // wiped). Non-fatal, like every split-horizon mutation.
        if let Some(domain) = self.get_cfg(KEY_CERT_DOMAIN).await? {
            self.delete_system_record(&domain).await;
        }

        // Drop the stored certificate. The ACME account secret is kept on purpose
        // (reused on re-enable), so it is NOT deleted here.
        self.secrets
            .delete(SECRET_CERT_CHAIN)
            .await
            .map_err(AppError::Internal)?;
        self.secrets
            .delete(SECRET_CERT_KEY)
            .await
            .map_err(AppError::Internal)?;

        for key in [
            KEY_CERT_DOMAIN,
            KEY_CERT_NOT_AFTER,
            KEY_TLS_PHASE,
            KEY_TLS_LAST_ERROR,
        ] {
            self.config.delete(key).await.map_err(AppError::Internal)?;
        }

        tracing::info!("tore down TLS provisioning; reverted :443 to placeholder cert");
        Ok(())
    }
}
