//! Dynamic DNS (DDNS) — keeps a public **A** record pointing at the home WAN IP
//! so the canonical FQDN resolves to the Pi, and (in a later commit) publishes
//! `_acme-challenge` TXT records for ACME DNS-01.
//!
//! ## Shape
//!
//! ```text
//! DdnsUpdateRunner  ──(admin auth ctx)──▶  DdnsService  ──▶  DnsProvider
//!  (background tick)                       (auth-gated)       (bridge | cloudflare)
//! ```
//!
//! The [`DdnsUpdateRunner`](runner::DdnsUpdateRunner) holds only
//! `Arc<dyn DdnsService>` and calls it under an admin context — it never touches
//! repositories or providers directly (see `.agents/architecture.md`). The
//! [`DdnsService`] is the auth-and-persistence chokepoint: every method opens
//! with [`auth_context::require_admin`]. Providers ([`DnsProvider`]) are pure
//! HTTP clients, **bound to their target at construction**, rebuilt by the
//! service from stored config + secrets per operation.
//!
//! ## Storage
//!
//! Non-secret state lives in `system_config` (provider choice, install id,
//! assigned FQDN, region, selected bridge endpoint, last-published IP, BYOD
//! domain + zone). Secrets live in the on-Pi [`SecretStore`] under `ddns/…`
//! (Ed25519 signing seed, bridge bearer token, Cloudflare API token).

pub mod bridge;
pub mod cloudflare;
pub mod doh;
pub mod provider;
pub mod public_ip;
pub mod region;
pub mod runner;

#[cfg(test)]
mod tests;

use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use wardnet_common::api::{DdnsResolutionCheckResponse, DdnsResolutionVerdict};
use wardnetd_data::repository::SystemConfigRepository;

use crate::auth_context;
use crate::error::AppError;
use crate::secret_store::SecretStore;

use self::bridge::BridgeProvider;
use self::cloudflare::CloudflareProvider;
use self::provider::DnsProvider;

// ── system_config keys (non-secret) ────────────────────────────────────────────
const KEY_PROVIDER: &str = "ddns_provider";
const KEY_INSTALL_ID: &str = "ddns_install_id";
const KEY_SUBDOMAIN: &str = "ddns_subdomain";
const KEY_REGION: &str = "ddns_region";
const KEY_BRIDGE_BASE_URL: &str = "ddns_bridge_base_url";
const KEY_LAST_IP: &str = "ddns_last_public_ip";
const KEY_DOMAIN: &str = "ddns_domain";
const KEY_CF_ZONE_ID: &str = "ddns_cf_zone_id";

// ── secret-store paths ─────────────────────────────────────────────────────────
const SECRET_BRIDGE_SIGNING_KEY: &str = "ddns/bridge/signing_key";
const SECRET_BRIDGE_BEARER: &str = "ddns/bridge/bearer_token";
const SECRET_CF_TOKEN: &str = "ddns/cloudflare/api_token";

const PROVIDER_BRIDGE: &str = "bridge";
const PROVIDER_CLOUDFLARE: &str = "cloudflare";

/// Outcome of a successful bridge registration, surfaced to the wizard (C9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdnsRegistration {
    /// The FQDN the bridge assigned, e.g. `happy-einstein.my.us.wardnet.network`.
    pub subdomain: String,
    /// The bridge's region label (display only).
    pub region: String,
}

/// Current DDNS state, surfaced to Settings / status views (C10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdnsStatus {
    /// `None` when DDNS is not configured; otherwise `"bridge"` or `"cloudflare"`.
    pub provider: Option<String>,
    /// The active public hostname (bridge subdomain or BYOD domain), if any.
    pub fqdn: Option<String>,
    /// The IP last published by the daemon, if any.
    pub last_public_ip: Option<String>,
}

/// Auth-gated DDNS operations. Every method requires an admin context.
#[async_trait]
pub trait DdnsService: Send + Sync {
    /// Probe regions, register a fresh installation on the best bridge under
    /// `name`, and persist its identity (config + secrets). Used by the wizard.
    async fn register_with_bridge(&self, name: String) -> Result<DdnsRegistration, AppError>;

    /// Check whether `name` is available on the best-latency bridge.
    async fn check_name_available(&self, name: String) -> Result<bool, AppError>;

    /// Configure the **BYOD-Cloudflare** provider for a domain the operator
    /// controls. Resolves the zone id from `domain` (which also validates
    /// `token`), persists provider identity (config + token secret), and returns
    /// the active hostname. Used by the wizard's BYOD path.
    async fn configure_cloudflare(
        &self,
        token: String,
        domain: String,
    ) -> Result<DdnsRegistration, AppError>;

    /// Discover the WAN IP and, if it changed since the last publish, push it
    /// through the active provider. Returns the published IP, or `None` when
    /// DDNS is unconfigured or the IP is unchanged. Called by the runner.
    async fn refresh_public_ip(&self) -> Result<Option<Ipv4Addr>, AppError>;

    /// Read the current DDNS status.
    async fn status(&self) -> Result<DdnsStatus, AppError>;

    /// Tear down the active provider and return DDNS to the unconfigured state:
    /// best-effort remove the upstream presence (bridge install / Cloudflare A
    /// record), then wipe all DDNS config keys + secrets. Idempotent — `Ok(())`
    /// when nothing is configured. The upstream delete is **non-fatal**: a dead
    /// backend is logged, never trapping the operator in a configured state.
    /// The caller is responsible for the TLS-side teardown (cert + serving), see
    /// [`TlsService::teardown`](crate::tls::TlsService::teardown).
    async fn teardown(&self) -> Result<(), AppError>;

    /// Resolve the active FQDN through **external** public resolvers (deliberately
    /// bypassing the local split-horizon override) and compare the result to the
    /// last published IP. Returns [`DdnsResolutionVerdict::NotConfigured`] when no
    /// provider is active.
    async fn resolution_check(&self) -> Result<DdnsResolutionCheckResponse, AppError>;

    /// Publish the ACME DNS-01 `_acme-challenge` TXT record for the active
    /// installation through the configured provider. Errors with
    /// [`AppError::Conflict`] when DDNS is unconfigured — a challenge can't be
    /// published without a provider. Called by the TLS service during
    /// certificate issuance.
    async fn set_acme_challenge(&self, value: &str) -> Result<(), AppError>;

    /// Remove the `_acme-challenge` TXT record. Idempotent at the provider
    /// level (absence is success). Errors with [`AppError::Conflict`] when DDNS
    /// is unconfigured. Called by the TLS service to clean up after issuance.
    async fn clear_acme_challenge(&self) -> Result<(), AppError>;
}

/// Tunable base URLs, overridable in tests to point at wiremock servers.
pub(crate) struct DdnsSettings {
    /// Region catalog as `(slug, base_url)` pairs.
    region_catalog: Vec<(String, String)>,
    /// Public-IP echo endpoints, tried in order.
    echo_endpoints: Vec<String>,
    /// Cloudflare API base URL.
    cf_base_url: String,
    /// DoH-JSON resolver URLs for the external resolution check, tried in order.
    doh_resolvers: Vec<String>,
}

impl Default for DdnsSettings {
    fn default() -> Self {
        Self {
            region_catalog: region::REGION_CATALOG
                .iter()
                .map(|e| (e.slug.to_owned(), e.base_url.to_owned()))
                .collect(),
            echo_endpoints: public_ip::ECHO_ENDPOINTS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            cf_base_url: cloudflare::CF_API_BASE.to_owned(),
            doh_resolvers: doh::DOH_RESOLVERS.iter().map(|s| (*s).to_owned()).collect(),
        }
    }
}

/// The concrete [`DdnsService`].
pub struct DdnsServiceImpl {
    config: Arc<dyn SystemConfigRepository>,
    secrets: Arc<dyn SecretStore>,
    http: reqwest::Client,
    settings: DdnsSettings,
}

impl DdnsServiceImpl {
    /// Build the service with the production region catalog and echo endpoints.
    #[must_use]
    pub fn new(config: Arc<dyn SystemConfigRepository>, secrets: Arc<dyn SecretStore>) -> Self {
        Self::with_settings(config, secrets, DdnsSettings::default())
    }

    pub(crate) fn with_settings(
        config: Arc<dyn SystemConfigRepository>,
        secrets: Arc<dyn SecretStore>,
        settings: DdnsSettings,
    ) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client builds with static config");
        Self {
            config,
            secrets,
            http,
            settings,
        }
    }

    // ── config / secret helpers ────────────────────────────────────────────

    async fn get_cfg(&self, key: &str) -> Result<Option<String>, AppError> {
        self.config.get(key).await.map_err(AppError::Internal)
    }

    async fn set_cfg(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.config
            .set(key, value)
            .await
            .map_err(AppError::Internal)
    }

    async fn get_secret_string(&self, path: &str) -> Result<String, AppError> {
        let bytes = self
            .secrets
            .get(path)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing secret at {path}")))?;
        String::from_utf8(bytes)
            .map_err(|_| AppError::Internal(anyhow::anyhow!("secret at {path} is not UTF-8")))
    }

    /// Build the active provider from stored config + secrets, or `None` when
    /// DDNS is unconfigured. The provider is bound to its target so trait calls
    /// carry only the dynamic payload.
    ///
    /// Constructed per call **by design**: the reads are local (`SQLite` +
    /// secret-store file), cheap at the runner's 5-minute cadence, and rebuilding
    /// each time is what lets a provider switch (C10) take effect without any
    /// cache-invalidation plumbing. All providers share the one pooled
    /// `self.http` client, so no connection state is discarded.
    async fn build_provider(&self) -> Result<Option<Box<dyn DnsProvider>>, AppError> {
        match self.get_cfg(KEY_PROVIDER).await?.as_deref() {
            Some(PROVIDER_BRIDGE) => {
                let install_id = self.require_cfg(KEY_INSTALL_ID).await?;
                let base_url = self.require_cfg(KEY_BRIDGE_BASE_URL).await?;
                let bearer = self.get_secret_string(SECRET_BRIDGE_BEARER).await?;
                let seed = self.require_signing_seed().await?;
                Ok(Some(Box::new(BridgeProvider::new(
                    self.http.clone(),
                    base_url,
                    install_id,
                    bearer,
                    seed,
                ))))
            }
            Some(PROVIDER_CLOUDFLARE) => {
                let domain = self.require_cfg(KEY_DOMAIN).await?;
                let zone_id = self.require_cfg(KEY_CF_ZONE_ID).await?;
                let token = self.get_secret_string(SECRET_CF_TOKEN).await?;
                let provider = CloudflareProvider::new_with_base_url(
                    self.http.clone(),
                    &token,
                    &zone_id,
                    &domain,
                    &self.settings.cf_base_url,
                );
                Ok(Some(Box::new(provider)))
            }
            _ => Ok(None),
        }
    }

    async fn require_cfg(&self, key: &str) -> Result<String, AppError> {
        self.get_cfg(key)
            .await?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing config key {key}")))
    }

    async fn require_signing_seed(&self) -> Result<[u8; 32], AppError> {
        let bytes = self
            .secrets
            .get(SECRET_BRIDGE_SIGNING_KEY)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing DDNS signing key")))?;
        bytes
            .try_into()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("DDNS signing key is not 32 bytes")))
    }

    /// The active public hostname (bridge subdomain or BYOD domain), if any.
    async fn active_fqdn(&self, provider: Option<&str>) -> Result<Option<String>, AppError> {
        match provider {
            Some(PROVIDER_BRIDGE) => self.get_cfg(KEY_SUBDOMAIN).await,
            Some(PROVIDER_CLOUDFLARE) => self.get_cfg(KEY_DOMAIN).await,
            _ => Ok(None),
        }
    }

    /// Delete the config keys + secrets specific to one provider `kind`. Used by
    /// [`Self::teardown`] (full wipe) and by the provider-switch path (clearing a
    /// superseded *other* provider's residue). Shared keys (`KEY_PROVIDER`,
    /// `KEY_LAST_IP`) are NOT touched here — the switch path keeps them for the
    /// new provider; teardown clears them separately.
    async fn clear_provider_state(&self, kind: &str) -> Result<(), AppError> {
        let (keys, secrets): (&[&str], &[&str]) = match kind {
            PROVIDER_BRIDGE => (
                &[
                    KEY_INSTALL_ID,
                    KEY_SUBDOMAIN,
                    KEY_REGION,
                    KEY_BRIDGE_BASE_URL,
                ],
                &[SECRET_BRIDGE_SIGNING_KEY, SECRET_BRIDGE_BEARER],
            ),
            PROVIDER_CLOUDFLARE => (&[KEY_DOMAIN, KEY_CF_ZONE_ID], &[SECRET_CF_TOKEN]),
            _ => (&[], &[]),
        };
        for key in keys {
            self.config.delete(key).await.map_err(AppError::Internal)?;
        }
        for path in secrets {
            self.secrets
                .delete(path)
                .await
                .map_err(AppError::Internal)?;
        }
        Ok(())
    }

    /// After a successful provider (re)configuration, deregister a now-superseded
    /// prior provider so its public record is not orphaned — the "new-first, then
    /// best-effort teardown-old" switch policy. Skipped when the target is
    /// unchanged (same kind + same FQDN, e.g. re-saving the same Cloudflare
    /// domain) so we never delete a record we just re-published.
    ///
    /// Best-effort throughout: the new provider is already live, so neither the
    /// upstream delete nor the residue cleanup may surface as an error.
    async fn teardown_superseded(
        &self,
        old_provider: Option<Box<dyn DnsProvider>>,
        old_kind: Option<&str>,
        old_fqdn: Option<&str>,
        new_kind: &str,
        new_fqdn: &str,
    ) {
        let superseded = old_kind != Some(new_kind) || old_fqdn != Some(new_fqdn);
        if !superseded {
            return;
        }
        if let Some(provider) = old_provider {
            let old_fqdn = old_fqdn.unwrap_or("?");
            match provider.teardown().await {
                Ok(()) => tracing::info!(
                    old_fqdn,
                    "deregistered superseded DDNS provider after switch: old_fqdn={old_fqdn}"
                ),
                Err(e) => tracing::warn!(
                    error = %e,
                    old_fqdn,
                    "failed to deregister superseded DDNS provider old_fqdn={old_fqdn}; its old public record may linger: {e}"
                ),
            }
        }
        // Cross-kind switch leaves the old kind's config/secret residue behind
        // (the new provider writes different keys); clear it. Same-kind switches
        // overwrote their own keys already.
        if let Some(old) = old_kind
            && old != new_kind
            && let Err(e) = self.clear_provider_state(old).await
        {
            tracing::warn!(error = %e, "failed to clear superseded DDNS provider residue: {e}");
        }
    }
}

/// Whether `name` is a syntactically valid bridge subdomain label.
///
/// **Security boundary:** `name` is interpolated into the bridge request URL
/// path (`/v1/names/{name}/available`, `/v1/register`), so it must be validated
/// **before** building that URL — an unchecked `/`, `?`, `.` or whitespace would
/// let an admin reshape the request path. The bridge enforces the authoritative
/// rules (incl. the reserved-name list) and is the source of truth; this is the
/// daemon-side guard against path injection, mirroring the bridge's charset and
/// length constraints (`source/bridge/src/api/validation.rs`).
fn is_valid_bridge_name(name: &str) -> bool {
    let len = name.len();
    (3..=32).contains(&len)
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[async_trait]
impl DdnsService for DdnsServiceImpl {
    async fn register_with_bridge(&self, name: String) -> Result<DdnsRegistration, AppError> {
        auth_context::require_admin()?;

        // Capture the prior provider (bound to its old identity) before any
        // overwrite, so a switch can deregister it after the new one commits. A
        // half-written prior config must never block a fresh registration, so a
        // build failure is treated as "no old provider" rather than propagated.
        let old_provider = self.build_provider().await.ok().flatten();
        let old_kind = self.get_cfg(KEY_PROVIDER).await?;
        let old_fqdn = self.active_fqdn(old_kind.as_deref()).await?;

        if !is_valid_bridge_name(&name) {
            return Err(AppError::BadRequest(
                "hostname must be 3–32 chars of lowercase letters, digits, and hyphens, \
                 not starting or ending with a hyphen"
                    .to_owned(),
            ));
        }

        let region = region::select_best(&self.http, &self.settings.region_catalog)
            .await
            .map_err(|e| AppError::UpstreamUnavailable(e.to_string()))?;

        let identity = bridge::register_install(&self.http, &region.base_url, &name)
            .await
            .map_err(|e| AppError::UpstreamUnavailable(e.to_string()))?;

        // Secrets first, so a partial failure never leaves a published identity
        // without its signing material.
        self.secrets
            .put(SECRET_BRIDGE_SIGNING_KEY, &identity.signing_key_seed)
            .await
            .map_err(AppError::Internal)?;
        self.secrets
            .put(SECRET_BRIDGE_BEARER, identity.bearer_token.as_bytes())
            .await
            .map_err(AppError::Internal)?;

        self.set_cfg(KEY_PROVIDER, PROVIDER_BRIDGE).await?;
        self.set_cfg(KEY_INSTALL_ID, &identity.install_id).await?;
        self.set_cfg(KEY_SUBDOMAIN, &identity.subdomain).await?;
        self.set_cfg(KEY_REGION, &identity.region).await?;
        self.set_cfg(KEY_BRIDGE_BASE_URL, &region.base_url).await?;

        tracing::info!(
            subdomain = %identity.subdomain,
            region = %identity.region,
            "registered DDNS installation with bridge"
        );

        self.teardown_superseded(
            old_provider,
            old_kind.as_deref(),
            old_fqdn.as_deref(),
            PROVIDER_BRIDGE,
            &identity.subdomain,
        )
        .await;

        Ok(DdnsRegistration {
            subdomain: identity.subdomain,
            region: identity.region,
        })
    }

    async fn check_name_available(&self, name: String) -> Result<bool, AppError> {
        auth_context::require_admin()?;
        // Reject invalid names locally (no network call) — both to avoid path
        // injection into the bridge URL and because an invalid name is, by
        // definition, not an available one (mirrors the bridge's own behaviour).
        if !is_valid_bridge_name(&name) {
            return Ok(false);
        }
        let region = region::select_best(&self.http, &self.settings.region_catalog)
            .await
            .map_err(|e| AppError::UpstreamUnavailable(e.to_string()))?;
        bridge::check_name_available(&self.http, &region.base_url, &name)
            .await
            .map_err(|e| AppError::UpstreamUnavailable(e.to_string()))
    }

    async fn configure_cloudflare(
        &self,
        token: String,
        domain: String,
    ) -> Result<DdnsRegistration, AppError> {
        auth_context::require_admin()?;

        // Capture the prior provider before any overwrite (see register_with_bridge).
        let old_provider = self.build_provider().await.ok().flatten();
        let old_kind = self.get_cfg(KEY_PROVIDER).await?;
        let old_fqdn = self.active_fqdn(old_kind.as_deref()).await?;

        // Resolving the zone id both maps domain → zone and validates the token.
        // `lookup_zone_id` already classifies the failure: a rejected token or an
        // uncovered domain is `BadRequest`, only a transport failure is
        // `UpstreamUnavailable` — so propagate it as-is.
        let zone_id =
            cloudflare::lookup_zone_id(&self.http, &self.settings.cf_base_url, &token, &domain)
                .await?;

        // Non-secret identity first, the credential **last** — the opposite
        // order to `register_with_bridge`, and deliberately so. There the bridge
        // signing key is irreplaceable (the bridge mints it once), so it must be
        // persisted before anything else can orphan it. Here the token is
        // user-supplied and re-enterable, so we never want to persist the
        // credential until the identity it belongs to is fully committed: a
        // transient DB failure then leaves a `configured-but-missing-token` state
        // that surfaces an error and self-heals on retry, rather than an
        // orphaned secret with no owning identity.
        self.set_cfg(KEY_PROVIDER, PROVIDER_CLOUDFLARE).await?;
        self.set_cfg(KEY_DOMAIN, &domain).await?;
        self.set_cfg(KEY_CF_ZONE_ID, &zone_id).await?;

        self.secrets
            .put(SECRET_CF_TOKEN, token.as_bytes())
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(%domain, "configured BYOD Cloudflare DDNS provider");

        self.teardown_superseded(
            old_provider,
            old_kind.as_deref(),
            old_fqdn.as_deref(),
            PROVIDER_CLOUDFLARE,
            &domain,
        )
        .await;

        Ok(DdnsRegistration {
            subdomain: domain,
            region: PROVIDER_CLOUDFLARE.to_owned(),
        })
    }

    async fn refresh_public_ip(&self) -> Result<Option<Ipv4Addr>, AppError> {
        auth_context::require_admin()?;

        let Some(provider) = self.build_provider().await? else {
            // Unconfigured: make zero network calls.
            return Ok(None);
        };

        let ip = public_ip::discover_from(&self.http, &self.echo_endpoint_refs())
            .await
            .map_err(|e| AppError::UpstreamUnavailable(e.to_string()))?;

        // Compare by parsed value so a reformatted/whitespace-padded stored
        // value can't mask an unchanged IP (or force a spurious republish).
        let last_ip = self
            .get_cfg(KEY_LAST_IP)
            .await?
            .and_then(|s| s.parse::<Ipv4Addr>().ok());
        if last_ip == Some(ip) {
            return Ok(None); // unchanged
        }

        provider
            .upsert_a(ip)
            .await
            .map_err(|e| AppError::UpstreamUnavailable(e.to_string()))?;
        self.set_cfg(KEY_LAST_IP, &ip.to_string()).await?;

        tracing::info!(%ip, "published DDNS A record");
        Ok(Some(ip))
    }

    async fn status(&self) -> Result<DdnsStatus, AppError> {
        auth_context::require_admin()?;
        let provider = self.get_cfg(KEY_PROVIDER).await?;
        let fqdn = self.active_fqdn(provider.as_deref()).await?;
        let last_public_ip = self.get_cfg(KEY_LAST_IP).await?;
        Ok(DdnsStatus {
            provider,
            fqdn,
            last_public_ip,
        })
    }

    async fn teardown(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let Some(kind) = self.get_cfg(KEY_PROVIDER).await? else {
            return Ok(()); // already unconfigured — idempotent
        };

        // Remove the upstream presence first, while the provider's config +
        // secrets are still present to build it. Non-fatal: a dead backend must
        // not trap the operator configured, so log and continue to the wipe.
        if let Some(provider) = self.build_provider().await?
            && let Err(e) = provider.teardown().await
        {
            tracing::warn!(
                error = %e,
                "failed to remove upstream DDNS record on teardown: {e}; wiping local state anyway"
            );
        }

        self.clear_provider_state(&kind).await?;
        self.config
            .delete(KEY_PROVIDER)
            .await
            .map_err(AppError::Internal)?;
        self.config
            .delete(KEY_LAST_IP)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!("tore down DDNS provider; public hostname released");
        Ok(())
    }

    async fn resolution_check(&self) -> Result<DdnsResolutionCheckResponse, AppError> {
        auth_context::require_admin()?;

        let status = self.status().await?;
        let Some(fqdn) = status.fqdn else {
            return Ok(DdnsResolutionCheckResponse {
                verdict: DdnsResolutionVerdict::NotConfigured,
                fqdn: None,
                expected_ip: None,
                resolved_ips: Vec::new(),
            });
        };

        // Query the fixed public resolvers (by IP, bypassing split-horizon) in
        // order; the first definitive answer wins. Only if every resolver fails
        // on transport do we report the check itself as unavailable.
        let mut resolved: Option<Vec<Ipv4Addr>> = None;
        let mut last_err: Option<String> = None;
        for resolver in &self.settings.doh_resolvers {
            match doh::resolve_a(&self.http, resolver, &fqdn).await {
                Ok(ips) => {
                    resolved = Some(ips);
                    break;
                }
                Err(e) => last_err = Some(e.to_string()),
            }
        }
        let resolved_ips = resolved.ok_or_else(|| {
            AppError::UpstreamUnavailable(format!(
                "no public resolver could be reached for the resolution check: {}",
                last_err.unwrap_or_else(|| "unknown error".to_owned())
            ))
        })?;

        let expected = status
            .last_public_ip
            .as_deref()
            .and_then(|s| s.parse::<Ipv4Addr>().ok());

        // `Pending` covers both "no A record visible yet" and "nothing published
        // yet to compare against" — the benign states in the propagation window.
        let verdict = match (expected, resolved_ips.is_empty()) {
            (_, true) | (None, false) => DdnsResolutionVerdict::Pending,
            (Some(exp), false) => {
                if resolved_ips.contains(&exp) {
                    DdnsResolutionVerdict::Match
                } else {
                    DdnsResolutionVerdict::Mismatch
                }
            }
        };

        Ok(DdnsResolutionCheckResponse {
            verdict,
            fqdn: Some(fqdn),
            expected_ip: status.last_public_ip,
            resolved_ips: resolved_ips.iter().map(ToString::to_string).collect(),
        })
    }

    async fn set_acme_challenge(&self, value: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;
        let provider = self.build_provider().await?.ok_or_else(|| {
            AppError::Conflict("DDNS is not configured — cannot publish ACME challenge".to_owned())
        })?;
        provider
            .set_txt(value)
            .await
            .map_err(|e| AppError::UpstreamUnavailable(e.to_string()))
    }

    async fn clear_acme_challenge(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        let provider = self.build_provider().await?.ok_or_else(|| {
            AppError::Conflict("DDNS is not configured — cannot clear ACME challenge".to_owned())
        })?;
        provider
            .delete_txt()
            .await
            .map_err(|e| AppError::UpstreamUnavailable(e.to_string()))
    }
}

impl DdnsServiceImpl {
    fn echo_endpoint_refs(&self) -> Vec<&str> {
        self.settings
            .echo_endpoints
            .iter()
            .map(String::as_str)
            .collect()
    }
}
