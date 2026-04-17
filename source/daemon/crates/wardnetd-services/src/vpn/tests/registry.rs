use std::sync::Arc;

use async_trait::async_trait;
use wardnet_common::vpn_provider::{
    CountryInfo, ProviderAuthMethod, ProviderCredentials, ProviderInfo, ServerFilter, ServerInfo,
};

use crate::vpn::provider::VpnProvider;
use crate::vpn::registry::{EnabledProviders, VpnProviderRegistry};

/// Minimal mock VPN provider for registry tests.
struct FakeProvider {
    info: ProviderInfo,
}

impl FakeProvider {
    fn new(id: &str, name: &str) -> Self {
        Self {
            info: ProviderInfo {
                id: id.to_owned(),
                name: name.to_owned(),
                auth_methods: vec![ProviderAuthMethod::Token],
                icon_url: None,
                website_url: None,
                credentials_hint: None,
            },
        }
    }
}

#[async_trait]
impl VpnProvider for FakeProvider {
    fn info(&self) -> ProviderInfo {
        self.info.clone()
    }

    async fn validate_credentials(
        &self,
        _credentials: &ProviderCredentials,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }

    async fn list_countries(
        &self,
        _credentials: &ProviderCredentials,
    ) -> anyhow::Result<Vec<CountryInfo>> {
        Ok(vec![])
    }

    async fn list_servers(
        &self,
        _credentials: &ProviderCredentials,
        _filter: &ServerFilter,
    ) -> anyhow::Result<Vec<ServerInfo>> {
        Ok(vec![])
    }

    async fn generate_config(
        &self,
        _credentials: &ProviderCredentials,
        _server: &ServerInfo,
    ) -> anyhow::Result<String> {
        Ok(String::new())
    }
}

fn with_nordvpn_disabled() -> EnabledProviders {
    let mut map = EnabledProviders::new();
    map.insert("nordvpn".to_owned(), false);
    map
}

#[test]
fn new_with_nordvpn_enabled_registers_provider() {
    // Empty map: providers default to enabled.
    let enabled = EnabledProviders::new();
    let registry = VpnProviderRegistry::new(&enabled);

    assert!(
        registry.get("nordvpn").is_some(),
        "NordVPN should be registered by default"
    );
    let info = registry.list();
    assert!(info.iter().any(|p| p.id == "nordvpn"));
}

#[test]
fn new_with_nordvpn_disabled_does_not_register() {
    let enabled = with_nordvpn_disabled();
    let registry = VpnProviderRegistry::new(&enabled);

    assert!(
        registry.get("nordvpn").is_none(),
        "NordVPN should not be registered when disabled"
    );
    assert!(registry.list().is_empty());
}

#[test]
fn register_and_get() {
    let enabled = with_nordvpn_disabled();
    let mut registry = VpnProviderRegistry::new(&enabled);

    registry.register(Arc::new(FakeProvider::new("alpha", "Alpha VPN")));

    let provider = registry.get("alpha");
    assert!(provider.is_some());
    assert_eq!(provider.unwrap().info().name, "Alpha VPN");
}

#[test]
fn get_returns_none_for_unknown_id() {
    let enabled = with_nordvpn_disabled();
    let registry = VpnProviderRegistry::new(&enabled);

    assert!(registry.get("nonexistent").is_none());
}

#[test]
fn list_returns_all_registered_providers() {
    let enabled = with_nordvpn_disabled();
    let mut registry = VpnProviderRegistry::new(&enabled);

    registry.register(Arc::new(FakeProvider::new("alpha", "Alpha VPN")));
    registry.register(Arc::new(FakeProvider::new("beta", "Beta VPN")));

    let list = registry.list();
    assert_eq!(list.len(), 2);
    let ids: Vec<&str> = list.iter().map(|p| p.id.as_str()).collect();
    assert!(ids.contains(&"alpha"));
    assert!(ids.contains(&"beta"));
}

#[test]
fn register_overwrites_existing_provider() {
    let enabled = with_nordvpn_disabled();
    let mut registry = VpnProviderRegistry::new(&enabled);

    registry.register(Arc::new(FakeProvider::new("test", "Version 1")));
    registry.register(Arc::new(FakeProvider::new("test", "Version 2")));

    let list = registry.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Version 2");
}
