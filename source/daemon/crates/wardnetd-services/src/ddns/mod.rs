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
use crate::cloud::{CloudError, DaemonIdentity, DdnsClient, TenantsClient, WardnetDnsProvider};
use crate::entitlement::Entitlement;
use crate::error::AppError;
use crate::secret_store::SecretStore;

use self::cloudflare::CloudflareProvider;
use self::provider::DnsProvider;
use self::region::RegionEndpoint;

// ── system_config keys (non-secret) ────────────────────────────────────────────
const KEY_PROVIDER: &str = "ddns_provider";
const KEY_TENANT_ID: &str = "ddns_tenant_id";
const KEY_NETWORK_ID: &str = "ddns_network_id";
const KEY_SLUG: &str = "ddns_slug";
const KEY_SUBDOMAIN: &str = "ddns_subdomain";
pub(crate) const KEY_REGION: &str = "ddns_region";
const KEY_LAST_IP: &str = "ddns_last_public_ip";
const KEY_DOMAIN: &str = "ddns_domain";
const KEY_CF_ZONE_ID: &str = "ddns_cf_zone_id";

// ── secret-store paths ─────────────────────────────────────────────────────────
/// The daemon's 32-byte Ed25519 seed — its cloud identity. Generated at enroll,
/// never leaves the Pi.
pub(crate) const SECRET_DAEMON_KEY: &str = "ddns/daemon/signing_key";
const SECRET_CF_TOKEN: &str = "ddns/cloudflare/api_token";

/// The wardnet-managed provider (enroll → network → ddns).
const PROVIDER_WARDNET: &str = "wardnet";
/// The Bring-Your-Own-Domain Cloudflare provider.
const PROVIDER_CLOUDFLARE: &str = "cloudflare";
/// The retired pre-mesh `bridge` provider. Any install still carrying it is
/// wiped on sight and must re-enroll (no migration — the auth model changed).
const PROVIDER_LEGACY_BRIDGE: &str = "bridge";

/// The parent domain wardnet vanity slugs hang off of: `<slug>.my.wardnet.services`.
const SUBDOMAIN_PARENT: &str = "my.wardnet.services";

/// Outcome of a successful bridge registration, surfaced to the wizard (C9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdnsRegistration {
    /// The FQDN the bridge assigned, e.g. `happy-einstein.my.wardnet.services`.
    pub subdomain: String,
    /// The bridge's region label (display only).
    pub region: String,
    /// `true` when the daemon joined an already-existing network of this
    /// account instead of creating a fresh one — the network's region, state,
    /// and name won over the request (display only).
    pub adopted: bool,
}

/// Current DDNS state, surfaced to Settings / status views (C10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdnsStatus {
    /// `None` when DDNS is not configured; otherwise `"wardnet"` or `"cloudflare"`.
    pub provider: Option<String>,
    /// The active public hostname (wardnet subdomain or BYOD domain), if any.
    pub fqdn: Option<String>,
    /// The IP last published by the daemon, if any.
    pub last_public_ip: Option<String>,
    /// `true` when the wardnet subscription is suspended (a token mint was
    /// refused) — the premium app surfaces are disabled. Always `false` for BYOD.
    pub suspended: bool,
}

/// Auth-gated DDNS operations. Every method requires an admin context.
#[async_trait]
pub trait DdnsService: Send + Sync {
    /// Request a one-time enrollment code be emailed to the wardnet account
    /// `email`. First step of the wizard's wardnet path. Stores nothing — the
    /// user then enters the emailed code into [`enroll`](Self::enroll).
    async fn request_enrollment_code(&self, email: String) -> Result<(), AppError>;

    /// Enroll against the one-time `code`: generate a fresh Ed25519 identity,
    /// bind it to the tenant, and persist the key + `tenant_id`. The daemon is
    /// now *enrolled* but has no network yet — [`register_network`] completes the
    /// wardnet provider.
    async fn enroll(&self, code: String) -> Result<(), AppError>;

    /// Check whether vanity `slug` is available (well-formed, unreserved, free).
    /// Requires a prior [`enroll`](Self::enroll) — availability is a tenant-scoped
    /// query.
    async fn check_slug(&self, slug: String) -> Result<bool, AppError>;

    /// Register a network under `slug` on the lowest-latency region, persist the
    /// wardnet provider identity (network id, slug, region, FQDN), and return the
    /// assigned `<slug>.my.wardnet.services` hostname. Requires a prior
    /// [`enroll`](Self::enroll).
    async fn register_network(
        &self,
        slug: String,
        display_name: Option<String>,
    ) -> Result<DdnsRegistration, AppError>;

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

    /// Cheap entitlement re-probe for the **suspended** state: force a token
    /// mint against the cloud purely for its side effect on the shared
    /// entitlement flag (a `200` calls `restore()`, a `403` keeps the box
    /// suspended). The runner calls this instead of [`refresh_public_ip`] while
    /// suspended, so the daemon self-heals the moment the operator resubscribes
    /// — without doing any of the heavier publish work that would `403` anyway.
    ///
    /// A no-op (`Ok(())`) when the active provider has no subscription to probe
    /// (BYOD-Cloudflare or unconfigured). The default impl is a no-op so mocks
    /// need not override it; only [`DdnsServiceImpl`] mints.
    async fn probe_entitlement(&self) -> Result<(), AppError> {
        Ok(())
    }

    /// Read the current DDNS status.
    async fn status(&self) -> Result<DdnsStatus, AppError>;

    /// Prime the shared [`Entitlement`]'s premium flag from the persisted
    /// provider config. Called once at startup, before the serving layer
    /// starts accepting connections, so a reboot doesn't transiently read the
    /// default `premium = false` for an already-premium box. A startup-only
    /// method that runs before the system is ready to authenticate anything,
    /// so it skips `require_admin()?` under the documented exception in
    /// `.agents/auth.md` (same category as `restore_tunnels`) — unlike
    /// [`probe_entitlement`](Self::probe_entitlement), which *does* require
    /// admin and is called under an explicit admin context by its runner.
    /// The default impl is a no-op so mocks need not override it; only
    /// [`DdnsServiceImpl`] tracks a provider to sync from.
    async fn sync_premium(&self) -> Result<(), AppError> {
        Ok(())
    }

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

    /// Publish the ACME DNS-01 `_acme-challenge` TXT record(s) for the active
    /// installation through the configured provider — one per value, all at the
    /// one challenge name and published together (a **per-user wildcard
    /// certificate** authorizes two SANs through the same name). Errors with
    /// [`AppError::Conflict`] when DDNS is unconfigured — a challenge can't be
    /// published without a provider. Called by the TLS service during
    /// certificate issuance.
    async fn set_acme_challenge(&self, values: &[String]) -> Result<(), AppError>;

    /// Remove the `_acme-challenge` TXT record. Idempotent at the provider
    /// level (absence is success). Errors with [`AppError::Conflict`] when DDNS
    /// is unconfigured. Called by the TLS service to clean up after issuance.
    async fn clear_acme_challenge(&self) -> Result<(), AppError>;
}

/// Tunable base URLs, overridable in tests to point at wiremock servers.
pub(crate) struct DdnsSettings {
    /// Per-region gateway catalog (control + health URLs), probed for selection.
    region_catalog: Vec<RegionEndpoint>,
    /// Global gateway base URL — fronts `tenants` (enroll / token / availability
    /// / networks under `/v1/…`).
    global_gateway_url: String,
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
            region_catalog: region::default_catalog(),
            global_gateway_url: region::GLOBAL_GATEWAY_URL.to_owned(),
            echo_endpoints: public_ip::ECHO_ENDPOINTS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            cf_base_url: cloudflare::CF_API_BASE.to_owned(),
            doh_resolvers: doh::DOH_RESOLVERS.iter().map(|s| (*s).to_owned()).collect(),
        }
    }
}

impl DdnsSettings {
    /// Defaults with the wardnet-cloud gateway URLs overridden per
    /// [`wardnet_common::config::DdnsWardnetConfig`], mirroring how
    /// [`VpnProviderRegistry`](crate::vpn::VpnProviderRegistry) takes an
    /// optional `nordvpn_api_url` override. Only the single built-in region's
    /// catalog entry (index 0) is replaced — deliberately indexed rather than
    /// mapped over the whole catalog, so a second region added later keeps
    /// its real URLs instead of silently inheriting the mock's; its slug is
    /// kept so `register_network`'s persisted `region` config value is
    /// unaffected.
    pub(crate) fn with_wardnet_overrides(
        gateway_url: Option<&str>,
        region_gateway_url: Option<&str>,
        region_health_url: Option<&str>,
    ) -> Self {
        let mut settings = Self::default();
        if let Some(url) = gateway_url {
            url.clone_into(&mut settings.global_gateway_url);
        }
        if let Some(region) = settings.region_catalog.first_mut() {
            if let Some(url) = region_gateway_url {
                url.clone_into(&mut region.gateway_base_url);
            }
            if let Some(url) = region_health_url {
                url.clone_into(&mut region.health_url);
            }
        }
        settings
    }

    /// The per-region gateway catalog. Exposed so the reverse-tunnel client
    /// (`cloud::tunneller_runner`) can resolve the same regional gateway the
    /// DDNS client dials — swapping `https://` for `wss://` and the path — from
    /// the region slug the enrollment persisted (issue #809).
    pub(crate) fn region_catalog(&self) -> &[RegionEndpoint] {
        &self.region_catalog
    }

    /// The global gateway base URL that fronts `tenants` (token minting). The
    /// reverse-tunnel client shares it to build its [`DaemonIdentity`].
    pub(crate) fn global_gateway_url(&self) -> &str {
        &self.global_gateway_url
    }
}

/// The concrete [`DdnsService`].
pub struct DdnsServiceImpl {
    config: Arc<dyn SystemConfigRepository>,
    secrets: Arc<dyn SecretStore>,
    http: reqwest::Client,
    settings: DdnsSettings,
    /// Process-wide entitlement handle, flipped by token mints (suspend on a
    /// `403`, restore on success) and read by the API/serving + runner layers.
    entitlement: Arc<Entitlement>,
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
            // Never honour an ambient `HTTP_PROXY`/`http_proxy`/`ALL_PROXY`
            // env var for cloud-API traffic: reqwest reads these by default,
            // and a CI runner's or operator's system-wide proxy silently
            // intercepting calls to the tenants/ddns gateways (or, in the
            // e2e harness, to `wardnet_cloud_mock`) is exactly the kind of
            // surprising, hard-to-diagnose failure this client should never
            // be subject to.
            .no_proxy()
            .build()
            .expect("reqwest client builds with static config");
        Self {
            config,
            secrets,
            http,
            settings,
            entitlement: Entitlement::shared(),
        }
    }

    /// The shared entitlement handle this service flips on token mints. The
    /// composition root clones it into [`AppState`] and the background runners so
    /// the whole daemon reads one suspended state.
    #[must_use]
    pub fn entitlement(&self) -> Arc<Entitlement> {
        self.entitlement.clone()
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
        match self.current_provider().await?.as_deref() {
            Some(PROVIDER_WARDNET) => {
                let network_id = self.require_cfg(KEY_NETWORK_ID).await?;
                let region = self.require_cfg(KEY_REGION).await?;
                let gateway_base = self.gateway_base_for_region(&region)?;
                let (tenants, identity) = self.build_identity().await?;
                let ddns = DdnsClient::new(self.http.clone(), gateway_base);
                Ok(Some(Box::new(WardnetDnsProvider::new(
                    ddns, tenants, identity, network_id,
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

    /// A fresh `tenants` client bound to the configured global endpoint. Cheap
    /// to build (shares the pooled `http`), so constructed per use.
    fn tenants_client(&self) -> Arc<TenantsClient> {
        Arc::new(TenantsClient::new(
            self.http.clone(),
            self.settings.global_gateway_url.clone(),
        ))
    }

    /// Load the persisted Ed25519 seed and build the daemon's cloud identity,
    /// returning it alongside the `tenants` client it mints tokens through.
    /// Errors with [`AppError::Conflict`] when not enrolled (no seed) — the wizard
    /// must enroll first.
    async fn build_identity(&self) -> Result<(Arc<TenantsClient>, Arc<DaemonIdentity>), AppError> {
        let bytes = self
            .secrets
            .get(SECRET_DAEMON_KEY)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Conflict("not enrolled - request a code and enroll first".to_owned())
            })?;
        let seed: [u8; 32] = bytes.try_into().map_err(|_| {
            AppError::Internal(anyhow::anyhow!("daemon signing key is not 32 bytes"))
        })?;
        let tenants = self.tenants_client();
        let identity = DaemonIdentity::from_seed(seed, tenants.clone(), self.entitlement.clone());
        Ok((tenants, identity))
    }

    /// Resolve a region slug to its gateway base URL from the catalog.
    fn gateway_base_for_region(&self, slug: &str) -> Result<String, AppError> {
        self.settings
            .region_catalog
            .iter()
            .find(|e| e.slug == slug)
            .map(|e| e.gateway_base_url.clone())
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("unknown DDNS region slug '{slug}'")))
    }

    /// The configured provider kind, treating the retired `bridge` value as
    /// unconfigured — and wiping its residue on sight (pre-GA: no migration, the
    /// operator re-enrolls).
    async fn current_provider(&self) -> Result<Option<String>, AppError> {
        match self.get_cfg(KEY_PROVIDER).await? {
            Some(kind) if kind == PROVIDER_LEGACY_BRIDGE => {
                self.wipe_legacy_bridge().await?;
                Ok(None)
            }
            other => Ok(other),
        }
    }

    /// Delete every key/secret left by the retired `bridge` provider so a stale
    /// identity never lingers or misreports status.
    async fn wipe_legacy_bridge(&self) -> Result<(), AppError> {
        const LEGACY_KEYS: &[&str] = &[
            "ddns_install_id",
            "ddns_bridge_base_url",
            "ddns_subdomain",
            "ddns_region",
            "ddns_provider",
            "ddns_last_public_ip",
        ];
        const LEGACY_SECRETS: &[&str] = &["ddns/bridge/signing_key", "ddns/bridge/bearer_token"];
        for key in LEGACY_KEYS {
            self.config.delete(key).await.map_err(AppError::Internal)?;
        }
        for path in LEGACY_SECRETS {
            self.secrets
                .delete(path)
                .await
                .map_err(AppError::Internal)?;
        }
        tracing::warn!(
            "wiped retired `bridge` DDNS identity; re-enroll via the wizard to use the new cloud"
        );
        Ok(())
    }

    /// The active public hostname (bridge subdomain or BYOD domain), if any.
    async fn active_fqdn(&self, provider: Option<&str>) -> Result<Option<String>, AppError> {
        match provider {
            Some(PROVIDER_WARDNET) => self.get_cfg(KEY_SUBDOMAIN).await,
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
            PROVIDER_WARDNET => (
                &[
                    KEY_TENANT_ID,
                    KEY_NETWORK_ID,
                    KEY_SLUG,
                    KEY_SUBDOMAIN,
                    KEY_REGION,
                ],
                &[SECRET_DAEMON_KEY],
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

/// Whether `slug` is a syntactically valid vanity slug.
///
/// **Security boundary:** the slug is interpolated into the tenants request URL
/// (`/v1/availability?slug={slug}`) and signed as part of the `PoP`
/// `path_and_query`, so it must be validated **before** building that URL — an
/// unchecked `/`, `?`, `.` or whitespace would let an admin reshape the request
/// path. The cloud enforces the authoritative rules (incl. the reserved-name
/// list); this is the daemon-side guard, mirroring the cloud's `[a-z0-9-]`, 3–32
/// constraints.
fn is_valid_slug(slug: &str) -> bool {
    let len = slug.len();
    (3..=32).contains(&len)
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Map a cloud-client error onto the service's [`AppError`] surface.
fn map_cloud_err(error: CloudError) -> AppError {
    match error {
        CloudError::EntitlementLost => {
            AppError::Forbidden("tenant subscription is not active".to_owned())
        }
        CloudError::BadRequest(detail) => AppError::BadRequest(detail),
        CloudError::Upstream(e) => AppError::UpstreamUnavailable(e.to_string()),
    }
}

#[async_trait]
impl DdnsService for DdnsServiceImpl {
    async fn request_enrollment_code(&self, email: String) -> Result<(), AppError> {
        auth_context::require_admin()?;
        let email = email.trim();
        // Minimal local sanity — the tenants service is authoritative.
        if email.is_empty() || !email.contains('@') {
            return Err(AppError::BadRequest(
                "a valid account email is required".to_owned(),
            ));
        }
        self.tenants_client()
            .request_enrollment_code(email)
            .await
            .map_err(map_cloud_err)
    }

    async fn enroll(&self, code: String) -> Result<(), AppError> {
        auth_context::require_admin()?;
        let code = code.trim();
        if code.is_empty() {
            return Err(AppError::BadRequest(
                "enrollment code is required".to_owned(),
            ));
        }

        // A fresh identity per enrollment — re-enrolling rebinds a new key.
        let mut seed = [0u8; 32];
        rand::fill(&mut seed);
        let tenants = self.tenants_client();
        let identity = DaemonIdentity::from_seed(seed, tenants.clone(), self.entitlement.clone());

        let tenant_id = tenants
            .enroll(code, identity.public_key_b64())
            .await
            .map_err(map_cloud_err)?;

        // Persist the key + tenant binding. The provider is *not* set to wardnet
        // yet — the daemon is enrolled but has no network until
        // `register_network` completes.
        self.secrets
            .put(SECRET_DAEMON_KEY, &seed)
            .await
            .map_err(AppError::Internal)?;
        self.set_cfg(KEY_TENANT_ID, &tenant_id).await?;

        tracing::info!(%tenant_id, "enrolled daemon identity with tenants: tenant_id={tenant_id}");
        Ok(())
    }

    async fn check_slug(&self, slug: String) -> Result<bool, AppError> {
        auth_context::require_admin()?;
        // Reject invalid slugs locally (no network call) — both to avoid path
        // injection into the request URL and because an invalid slug is, by
        // definition, unavailable (mirrors the cloud's own behaviour).
        if !is_valid_slug(&slug) {
            return Ok(false);
        }
        let (tenants, identity) = self.build_identity().await?;
        tenants
            .availability(&identity, &slug)
            .await
            .map_err(map_cloud_err)
    }

    async fn register_network(
        &self,
        slug: String,
        display_name: Option<String>,
    ) -> Result<DdnsRegistration, AppError> {
        auth_context::require_admin()?;
        if !is_valid_slug(&slug) {
            return Err(AppError::BadRequest(
                "slug must be 3–32 chars of lowercase letters, digits, and hyphens, \
                 not starting or ending with a hyphen"
                    .to_owned(),
            ));
        }

        let (tenants, identity) = self.build_identity().await?;

        // Capture the prior provider (bound to its old identity) before any
        // overwrite, so a switch can deregister it after the new one commits. A
        // half-written prior config must never block registration, so a build
        // failure is treated as "no old provider".
        let old_provider = self.build_provider().await.ok().flatten();
        let old_kind = self.current_provider().await?;
        let old_fqdn = self.active_fqdn(old_kind.as_deref()).await?;

        let region = region::select_best(&self.http, &self.settings.region_catalog)
            .await
            .map_err(|e| AppError::UpstreamUnavailable(e.to_string()))?;

        let network = tenants
            .register_network(&identity, &slug, display_name.as_deref(), &region.slug)
            .await
            .map_err(map_cloud_err)?;

        // The next token must be network-scoped (the daemon can now reach ddns).
        identity.forget_token();

        // Defend at write time: the response's region is server-controlled
        // (an ADOPTED network's region wins over our request) and every later
        // gateway resolution — IP reports, ACME, the tunnel — keys off the
        // persisted value strictly through the local catalog. Persisting a slug
        // the catalog cannot resolve would 200 the wizard and then silently
        // brick remote access on every background tick, so refuse it here,
        // before any config is written.
        self.gateway_base_for_region(&network.region).map_err(|_| {
            AppError::UpstreamUnavailable(format!(
                "your network '{}' lives in region '{}', which this wardnet build does not                  know — update wardnet and retry",
                network.slug, network.region,
            ))
        })?;

        let subdomain = format!("{}.{SUBDOMAIN_PARENT}", network.slug);
        self.set_cfg(KEY_NETWORK_ID, &network.network_id).await?;
        self.set_cfg(KEY_SLUG, &network.slug).await?;
        self.set_cfg(KEY_SUBDOMAIN, &subdomain).await?;
        // The RESPONSE's region, not our latency-based pick: when the cloud
        // adopts an existing network (same tenant re-registering its own slug),
        // that network's region is authoritative — IP reports and ACME calls
        // key their regional gateway off this value.
        self.set_cfg(KEY_REGION, &network.region).await?;
        self.set_cfg(KEY_PROVIDER, PROVIDER_WARDNET).await?;
        self.entitlement.set_premium(true);

        tracing::info!(
            %subdomain,
            region = %network.region,
            provisioning_state = %network.provisioning_state,
            adopted = network.adopted,
            "registered DDNS network: subdomain={subdomain}, region={region}, state={state},              adopted={adopted}",
            region = network.region,
            state = network.provisioning_state,
            adopted = network.adopted,
        );

        self.teardown_superseded(
            old_provider,
            old_kind.as_deref(),
            old_fqdn.as_deref(),
            PROVIDER_WARDNET,
            &subdomain,
        )
        .await;

        Ok(DdnsRegistration {
            subdomain,
            region: network.region,
            adopted: network.adopted,
        })
    }

    async fn configure_cloudflare(
        &self,
        token: String,
        domain: String,
    ) -> Result<DdnsRegistration, AppError> {
        auth_context::require_admin()?;

        // Capture the prior provider before any overwrite (see register_with_bridge).
        let old_provider = self.build_provider().await.ok().flatten();
        let old_kind = self.current_provider().await?;
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
        // Flip immediately once KEY_PROVIDER (the field `premium` mirrors) has
        // committed — not after the later, independently-fallible `KEY_DOMAIN`/
        // `KEY_CF_ZONE_ID`/secret writes, so a failure past this point can
        // never leave `premium` stale relative to the persisted provider.
        self.entitlement.set_premium(false);
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
            adopted: false,
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

    async fn probe_entitlement(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;

        // Only the wardnet provider carries a subscription. BYOD-Cloudflare and
        // the unconfigured state have nothing to probe — never suspended, so a
        // mint here would be meaningless (and `build_identity` would fail with
        // "not enrolled").
        if self.current_provider().await? != Some(PROVIDER_WARDNET.to_owned()) {
            return Ok(());
        }

        // Force a mint for its side effect only: `mint_token` flips the shared
        // entitlement flag (restore on `200`, re-suspend on `403`). We discard
        // the token. `EntitlementLost` is the expected "still suspended" outcome
        // — swallow it; surface only genuine transport failures so the runner
        // can log them.
        let (_, identity) = self.build_identity().await?;
        match identity.token().await {
            // Either the mint succeeded (the flag was restored inside the client)
            // or it `403`'d (still suspended, flag already set) — both leave the
            // shared entitlement correct, so the probe is done.
            Ok(_) | Err(CloudError::EntitlementLost) => Ok(()),
            Err(e) => Err(AppError::UpstreamUnavailable(e.to_string())),
        }
    }

    async fn sync_premium(&self) -> Result<(), AppError> {
        // Documented exception to the auth-guard rule (.agents/auth.md §Rules #2,
        // category (a): startup/restore): reconciles the cached premium flag with
        // the configured provider on startup and after provider changes, outside
        // any admin session.
        let provider = self.current_provider().await?;
        self.entitlement
            .set_premium(provider.as_deref() == Some(PROVIDER_WARDNET));
        Ok(())
    }

    async fn status(&self) -> Result<DdnsStatus, AppError> {
        auth_context::require_admin()?;
        let provider = self.current_provider().await?;
        let fqdn = self.active_fqdn(provider.as_deref()).await?;
        let last_public_ip = self.get_cfg(KEY_LAST_IP).await?;
        // Suspension only applies to the wardnet provider; BYOD has no subscription.
        let suspended =
            provider.as_deref() == Some(PROVIDER_WARDNET) && self.entitlement.is_suspended();
        Ok(DdnsStatus {
            provider,
            fqdn,
            last_public_ip,
            suspended,
        })
    }

    async fn teardown(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let Some(kind) = self.current_provider().await? else {
            return Ok(()); // already unconfigured (or legacy, now wiped) — idempotent
        };

        // Remove the upstream presence first, while the provider's config +
        // secrets are still present to build it. Non-fatal: neither a dead backend
        // nor a half-written/corrupt config (a build failure) may trap the
        // operator configured, so a build error is treated as "nothing to remove"
        // (`.ok().flatten()`) and we always continue to the local wipe.
        if let Some(provider) = self.build_provider().await.ok().flatten()
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
        // Flip immediately once KEY_PROVIDER (the field `premium` mirrors) is
        // gone — not after the independently-fallible `KEY_LAST_IP` delete, so
        // a failure past this point can never leave `premium` stale relative
        // to the persisted (now-absent) provider.
        self.entitlement.set_premium(false);
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

        // The wardnet provider's tenant record is a CNAME at the region's
        // Tunneller ingress (cloud ADR-0016): public DNS is EXPECTED to
        // resolve to the ingress, never this box's WAN address, so comparing
        // against `last_public_ip` would report `Mismatch` forever on a
        // healthy setup. Resolving AT ALL is the health signal — it proves
        // the tenant CNAME and the region's ingress record both exist. BYOD
        // keeps the WAN-IP comparison: the customer's own domain really is an
        // A record of their address.
        if status.provider.as_deref() == Some(PROVIDER_WARDNET) {
            let verdict = if resolved_ips.is_empty() {
                DdnsResolutionVerdict::Pending
            } else {
                DdnsResolutionVerdict::Match
            };
            return Ok(DdnsResolutionCheckResponse {
                verdict,
                fqdn: Some(fqdn),
                expected_ip: None,
                resolved_ips: resolved_ips.iter().map(ToString::to_string).collect(),
            });
        }

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

    async fn set_acme_challenge(&self, values: &[String]) -> Result<(), AppError> {
        auth_context::require_admin()?;
        let provider = self.build_provider().await?.ok_or_else(|| {
            AppError::Conflict("DDNS is not configured - cannot publish ACME challenge".to_owned())
        })?;
        provider
            .set_txt(values)
            .await
            .map_err(|e| AppError::UpstreamUnavailable(e.to_string()))
    }

    async fn clear_acme_challenge(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        let provider = self.build_provider().await?.ok_or_else(|| {
            AppError::Conflict("DDNS is not configured - cannot clear ACME challenge".to_owned())
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
