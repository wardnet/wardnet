use async_trait::async_trait;
use wardnet_common::vpn_provider::{
    CountryInfo, ProviderCredentials, ProviderInfo, ServerFilter, ServerInfo,
};

/// A pluggable VPN provider that can validate credentials, list servers,
/// and generate `WireGuard` configuration files.
///
/// Each provider implementation handles the API calls specific to that
/// VPN service (e.g. `NordVPN`, Mullvad). The provider trait is the
/// boundary between wardnet business logic and external VPN APIs.
#[async_trait]
pub trait VpnProvider: Send + Sync {
    /// Return metadata about this provider.
    fn info(&self) -> ProviderInfo;

    /// Validate that the given credentials are accepted by the provider.
    async fn validate_credentials(&self, credentials: &ProviderCredentials)
    -> anyhow::Result<bool>;

    /// List countries where this provider has servers.
    async fn list_countries(
        &self,
        credentials: &ProviderCredentials,
    ) -> anyhow::Result<Vec<CountryInfo>>;

    /// Fetch available servers, optionally filtered.
    async fn list_servers(
        &self,
        credentials: &ProviderCredentials,
        filter: &ServerFilter,
    ) -> anyhow::Result<Vec<ServerInfo>>;

    /// Resolve a server by hostname for dedicated IP or manual selection.
    /// Returns `None` if the provider does not support hostname resolution.
    async fn resolve_server(
        &self,
        _credentials: &ProviderCredentials,
        _hostname: &str,
    ) -> anyhow::Result<Option<ServerInfo>> {
        Ok(None)
    }

    /// Generate a `WireGuard` `.conf` string for connecting to the given server.
    ///
    /// The returned string is a complete `WireGuard` config that can be passed
    /// directly to `wireguard_config::parse()`.
    async fn generate_config(
        &self,
        credentials: &ProviderCredentials,
        server: &ServerInfo,
    ) -> anyhow::Result<String>;
}
