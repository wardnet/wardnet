use std::collections::HashMap;
use std::sync::Arc;

use wardnet_types::vpn_provider::ProviderInfo;

use crate::config::Config;
use crate::vpn_provider::VpnProvider;
use crate::vpn_provider_nordvpn::{NordVpnProvider, RealNordVpnApi};

/// Thread-safe registry of available VPN providers.
///
/// Owns a map of provider ID to `VpnProvider` trait objects. Built once at
/// startup from configuration and used by `ProviderService` for lookups.
pub struct VpnProviderRegistry {
    providers: HashMap<String, Arc<dyn VpnProvider>>,
}

impl VpnProviderRegistry {
    /// Create a new registry, constructing all built-in providers that are
    /// enabled in `config`.
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
        };

        // Register NordVPN provider.
        if config.is_provider_enabled("nordvpn") {
            let api = Arc::new(RealNordVpnApi::new());
            registry.register(Arc::new(NordVpnProvider::new(api)));
        }

        registry
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
