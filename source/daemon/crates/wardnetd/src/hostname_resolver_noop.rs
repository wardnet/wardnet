use async_trait::async_trait;

use crate::hostname_resolver::HostnameResolver;

/// No-op hostname resolver that always returns `None`.
///
/// Used in tests and when running with `--mock-network`.
pub struct NoopHostnameResolver;

#[async_trait]
impl HostnameResolver for NoopHostnameResolver {
    async fn resolve(&self, _ip: &str) -> Option<String> {
        None
    }
}
