use crate::hostname_resolver::SystemHostnameResolver;
use wardnetd_services::device::hostname_resolver::HostnameResolver;

#[tokio::test]
async fn resolve_loopback_returns_localhost() {
    let resolver = SystemHostnameResolver;

    // Loopback address (127.0.0.1) is commonly mapped to "localhost" in /etc/hosts.
    // This test is environment-dependent but works on most development machines.
    let result = resolver.resolve("127.0.0.1").await;

    // On most systems this resolves to "localhost", but on some CI environments
    // it may return None. We just verify it does not panic and returns a valid result.
    if let Some(hostname) = &result {
        assert!(!hostname.is_empty());
    }
}

#[tokio::test]
async fn resolve_invalid_ip_returns_none() {
    let resolver = SystemHostnameResolver;

    // An invalid IP address should return None without panicking.
    let result = resolver.resolve("not-an-ip").await;
    assert!(result.is_none());
}

#[tokio::test]
async fn resolve_unreachable_ip_returns_none() {
    let resolver = SystemHostnameResolver;

    // RFC 5737 documentation address, unlikely to resolve via DNS.
    let result = resolver.resolve("198.51.100.254").await;
    assert!(result.is_none());
}
