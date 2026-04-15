use std::net::SocketAddr;

use wardnet_types::dns::DnsConfig;

use crate::dns::server::{DnsServer, NoopDnsServer, UdpDnsServer};

// ---------------------------------------------------------------------------
// NoopDnsServer tests
// ---------------------------------------------------------------------------

#[test]
fn noop_server_is_not_running() {
    let server = NoopDnsServer;
    assert!(!server.is_running());
}

#[tokio::test]
async fn noop_server_start_and_stop() {
    let server = NoopDnsServer;
    server.start().await.unwrap();
    server.stop().await.unwrap();
    assert!(!server.is_running());
}

#[tokio::test]
async fn noop_server_flush_cache_returns_zero() {
    let server = NoopDnsServer;
    assert_eq!(server.flush_cache().await, 0);
}

#[tokio::test]
async fn noop_server_cache_size_returns_zero() {
    let server = NoopDnsServer;
    assert_eq!(server.cache_size().await, 0);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Bind address for tests: loopback with port 0 lets the OS pick an
/// ephemeral port, avoiding conflicts.
fn loopback_ephemeral() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
}

// ---------------------------------------------------------------------------
// UdpDnsServer start/stop tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn udp_server_start_sets_running_flag() {
    let config = DnsConfig::default();
    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());

    server.start().await.unwrap();
    assert!(server.is_running(), "server should be running after start");

    server.stop().await.unwrap();
}

#[tokio::test]
async fn udp_server_stop_clears_running_flag() {
    let config = DnsConfig::default();
    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());

    server.start().await.unwrap();
    assert!(server.is_running());

    server.stop().await.unwrap();

    // Give the spawned task a moment to complete.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !server.is_running(),
        "server should not be running after stop"
    );
}

#[tokio::test]
async fn udp_server_start_when_already_running() {
    let config = DnsConfig::default();
    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());

    server.start().await.unwrap();
    // Second start should be a no-op (returns Ok).
    server.start().await.unwrap();
    assert!(server.is_running());

    server.stop().await.unwrap();
}

#[tokio::test]
async fn udp_server_stop_when_not_running() {
    let config = DnsConfig::default();
    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());

    // Stop without start should be a no-op.
    server.stop().await.unwrap();
    assert!(!server.is_running());
}

#[tokio::test]
async fn udp_server_flush_cache() {
    let config = DnsConfig::default();
    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());

    server.start().await.unwrap();

    // Empty cache should return 0.
    let flushed = server.flush_cache().await;
    assert_eq!(flushed, 0, "empty cache should flush 0 entries");

    server.stop().await.unwrap();
}

#[tokio::test]
async fn udp_server_update_config() {
    let config = DnsConfig::default();
    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());

    // Update the config before starting -- should not panic or error.
    let mut new_config = DnsConfig::default();
    new_config.cache_size = 5_000;
    new_config.cache_ttl_max_secs = 3_600;
    server.update_config(new_config).await;

    // Update while running should also succeed.
    server.start().await.unwrap();

    let mut running_config = DnsConfig::default();
    running_config.cache_size = 20_000;
    server.update_config(running_config).await;

    server.stop().await.unwrap();
}

#[tokio::test]
async fn build_resolver_with_empty_upstreams_uses_cloudflare() {
    // When upstream_servers is empty, the server should still start
    // successfully because `build_resolver` falls back to Cloudflare.
    let mut config = DnsConfig::default();
    config.upstream_servers = vec![];

    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());

    server.start().await.unwrap();
    assert!(
        server.is_running(),
        "server should start even with empty upstreams (Cloudflare fallback)"
    );

    server.stop().await.unwrap();
}
