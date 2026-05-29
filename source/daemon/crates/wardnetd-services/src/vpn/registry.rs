use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use wardnet_common::tunnel::BestServerSelector;
use wardnet_common::vpn_provider::{ProviderCredentials, ProviderInfo, ServerFilter};

use crate::vpn::nordvpn::{HttpNordVpnApi, NordVpnProvider};
use crate::vpn::provider::VpnProvider;
use crate::vpn::resolver::{EmptyServerListError, ServerResolver};

/// Per-provider enable/disable flags passed from daemon configuration.
pub type EnabledProviders = HashMap<String, bool>;

/// Thread-safe registry of available VPN providers.
///
/// Owns a map of provider ID to `VpnProvider` trait objects. Built once at
/// startup from configuration and used by `VpnProviderService` for lookups.
pub struct VpnProviderRegistry {
    providers: HashMap<String, Arc<dyn VpnProvider>>,
}

impl VpnProviderRegistry {
    /// Create a new registry, constructing all built-in providers.
    ///
    /// `enabled` maps provider IDs to enabled/disabled flags. Providers not
    /// listed are treated as enabled.
    #[must_use]
    pub fn new(enabled: &EnabledProviders) -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
        };

        // Register NordVPN provider.
        if Self::is_enabled(enabled, "nordvpn") {
            let api = Arc::new(HttpNordVpnApi::new());
            registry.register(Arc::new(NordVpnProvider::new(api)));
        }

        registry
    }

    /// Check whether a provider is enabled. Returns `true` unless explicitly
    /// set to `false`.
    fn is_enabled(enabled: &EnabledProviders, id: &str) -> bool {
        enabled.get(id).copied().unwrap_or(true)
    }

    /// Register a provider. Overwrites any existing provider with the same ID.
    pub fn register(&mut self, provider: Arc<dyn VpnProvider>) {
        let id = provider.info().id;
        self.providers.insert(id, provider);
    }

    /// Look up a provider by ID. Returns `None` if no provider is registered
    /// with that identifier.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Arc<dyn VpnProvider>> {
        self.providers.get(id)
    }

    /// Return metadata for all registered providers.
    #[must_use]
    pub fn list(&self) -> Vec<ProviderInfo> {
        self.providers.values().map(|p| p.info()).collect()
    }
}

/// Reject hostnames that could be used for endpoint injection (e.g. containing
/// a colon that would override the port, or whitespace from a malformed response).
fn validate_hostname(hostname: &str) -> anyhow::Result<()> {
    if hostname.is_empty() || hostname.chars().any(|c| c == ':' || c.is_whitespace()) {
        anyhow::bail!("provider returned invalid hostname: {hostname:?}");
    }
    Ok(())
}

#[async_trait]
impl ServerResolver for VpnProviderRegistry {
    async fn resolve(
        &self,
        provider_id: &str,
        selector: &BestServerSelector,
        port: u16,
    ) -> anyhow::Result<Option<(String, String)>> {
        let Some(provider) = self.providers.get(provider_id) else {
            return Ok(None);
        };

        // NordVPN's recommendations endpoint is unauthenticated (public API).
        // The `list_servers` trait signature accepts credentials for providers
        // that require auth; we pass an empty token here because NordVPN ignores
        // it. A future provider that requires auth for listing must not use this
        // path — extend the trait with a credential-free method instead.
        let dummy = ProviderCredentials::Token {
            token: String::new(),
        };
        let filter = ServerFilter {
            country: Some(selector.country.clone()),
            max_load: None,
        };

        let servers = provider.list_servers(&dummy, &filter).await?;

        if servers.is_empty() {
            return Err(anyhow::Error::new(EmptyServerListError {
                country: selector.country.clone(),
                provider: provider_id.to_owned(),
            }));
        }

        let server = servers.iter().min_by_key(|s| s.load).expect("non-empty");
        validate_hostname(&server.hostname)?;
        let endpoint = format!("{}:{port}", server.hostname);
        Ok(Some((endpoint, server.name.clone())))
    }
}
