use async_trait::async_trait;
use wardnet_common::tunnel::BestServerSelector;

/// Sentinel error indicating the server list for the requested selector was
/// empty. `bring_up_core` downcasts to this to distinguish "no servers
/// available" (fatal bring-up failure) from a transient network error (fall
/// back to the stored endpoint).
#[derive(Debug, thiserror::Error)]
#[error("no servers found for country={country} via provider={provider}")]
pub struct EmptyServerListError {
    pub country: String,
    pub provider: String,
}

/// Queries a VPN provider for the best server matching a stored selector.
///
/// Implemented by [`VpnProviderRegistry`](super::registry::VpnProviderRegistry)
/// so that `TunnelServiceImpl` can re-resolve the endpoint on each bring-up
/// without holding a direct reference to provider APIs.
#[async_trait]
pub trait ServerResolver: Send + Sync {
    /// Return `(endpoint_host:port, human_readable_server_name)` for the best
    /// server matching `selector` on the given provider, or `Ok(None)` if the
    /// provider is not registered. Returns `Err` on network/API failure or
    /// (as [`EmptyServerListError`]) when the server list is empty.
    async fn resolve(
        &self,
        provider_id: &str,
        selector: &BestServerSelector,
        port: u16,
    ) -> anyhow::Result<Option<(String, String)>>;
}
