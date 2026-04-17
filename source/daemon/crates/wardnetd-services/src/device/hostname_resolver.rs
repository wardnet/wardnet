use async_trait::async_trait;

/// Resolves IP addresses to hostnames via reverse DNS.
#[async_trait]
pub trait HostnameResolver: Send + Sync {
    /// Attempt to resolve a hostname for the given IP address.
    ///
    /// Returns `None` if resolution fails or times out.
    async fn resolve(&self, ip: &str) -> Option<String>;
}
