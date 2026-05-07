//! Unit tests for [`UdpDnsServer`].
//!
//! Covers the lifecycle (start / stop / running flag), config update,
//! cache flush, and the small helper functions (`record_query`,
//! `duration_to_ms`, `upstream_label`). The full hot-path through
//! `handle_query` and the stop-drains-in-flight-handlers race are
//! exercised via the e2e suite (`dns-config.spec.ts`) — the toggle path
//! there does a synchronous start→stop→start cycle that this race
//! breaks without the fix.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use hickory_proto::rr::RecordType;
use wardnet_common::dns::{DnsConfig, DnsProtocol, UpstreamDns};
use wardnetd_services::dns::server::DnsServer;

use crate::dns::server::{UdpDnsServer, duration_to_ms, upstream_label};
use crate::tests::stubs::StubDnsFilterService;

fn loopback_ephemeral() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
}

fn stub_filter() -> Arc<dyn wardnetd_services::DnsFilterService> {
    Arc::new(StubDnsFilterService)
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_sets_running_flag() {
    let server =
        UdpDnsServer::with_bind_addr(DnsConfig::default(), loopback_ephemeral(), stub_filter());

    server.start().await.unwrap();
    assert!(server.is_running(), "server should be running after start");

    server.stop().await.unwrap();
}

#[tokio::test]
async fn stop_clears_running_flag() {
    let server =
        UdpDnsServer::with_bind_addr(DnsConfig::default(), loopback_ephemeral(), stub_filter());

    server.start().await.unwrap();
    assert!(server.is_running());
    server.stop().await.unwrap();

    // Spawned task takes a moment to wind down before flipping the flag.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !server.is_running(),
        "server should not be running after stop"
    );
}

#[tokio::test]
async fn second_start_is_a_noop() {
    let server =
        UdpDnsServer::with_bind_addr(DnsConfig::default(), loopback_ephemeral(), stub_filter());

    server.start().await.unwrap();
    // Second start is documented as a no-op (warns + returns Ok).
    server.start().await.unwrap();
    assert!(server.is_running());

    server.stop().await.unwrap();
}

#[tokio::test]
async fn stop_when_not_running_is_a_noop() {
    let server =
        UdpDnsServer::with_bind_addr(DnsConfig::default(), loopback_ephemeral(), stub_filter());

    server.stop().await.unwrap();
    assert!(!server.is_running());
}

#[tokio::test]
async fn restart_after_stop_works() {
    // Each `start()` must create a fresh `TaskTracker`. If the tracker
    // is reused without recreation, the second `tracker.spawn(...)` after
    // a `tracker.close()` would panic. Toggling the server quickly off
    // and on (the dns-config e2e path) needs this to be safe.
    let server =
        UdpDnsServer::with_bind_addr(DnsConfig::default(), loopback_ephemeral(), stub_filter());

    server.start().await.unwrap();
    server.stop().await.unwrap();

    // The drained tracker has been replaced; start() must succeed again.
    server.start().await.unwrap();
    assert!(server.is_running());
    server.stop().await.unwrap();
}

#[tokio::test]
async fn stop_is_idempotent_after_drain() {
    let server =
        UdpDnsServer::with_bind_addr(DnsConfig::default(), loopback_ephemeral(), stub_filter());

    server.start().await.unwrap();
    server.stop().await.unwrap();
    // Second stop on an already-stopped server is documented as a no-op.
    server.stop().await.unwrap();
}

#[tokio::test]
async fn flush_cache_returns_zero_on_empty() {
    let server =
        UdpDnsServer::with_bind_addr(DnsConfig::default(), loopback_ephemeral(), stub_filter());
    server.start().await.unwrap();

    let flushed = server.flush_cache().await;
    assert_eq!(flushed, 0);

    server.stop().await.unwrap();
}

#[tokio::test]
async fn update_config_works_before_and_after_start() {
    let server =
        UdpDnsServer::with_bind_addr(DnsConfig::default(), loopback_ephemeral(), stub_filter());

    // Pre-start: should not panic.
    server
        .update_config(DnsConfig {
            cache_size: 5_000,
            ..DnsConfig::default()
        })
        .await;

    server.start().await.unwrap();

    // Post-start: should also be fine.
    server
        .update_config(DnsConfig {
            cache_size: 20_000,
            ..DnsConfig::default()
        })
        .await;

    server.stop().await.unwrap();
}

#[tokio::test]
async fn empty_upstream_servers_falls_back_to_cloudflare() {
    // build_resolver inside the server treats `upstream_servers = []` as
    // "use Cloudflare" — exercising the start path under that fallback.
    let server = UdpDnsServer::with_bind_addr(
        DnsConfig {
            upstream_servers: vec![],
            ..DnsConfig::default()
        },
        loopback_ephemeral(),
        stub_filter(),
    );

    server.start().await.unwrap();
    assert!(server.is_running());
    server.stop().await.unwrap();
}

// ---------------------------------------------------------------------------
// Helper functions — pure and easy to assert on.
// ---------------------------------------------------------------------------

#[test]
fn duration_to_ms_converts_microseconds_with_fraction() {
    assert!((duration_to_ms(Duration::from_micros(1500)) - 1.5).abs() < 1e-9);
}

#[test]
fn duration_to_ms_zero() {
    assert!(duration_to_ms(Duration::from_secs(0)).abs() < 1e-9);
}

#[test]
fn duration_to_ms_seconds_round_trip() {
    let one_second = duration_to_ms(Duration::from_secs(1));
    assert!((one_second - 1000.0).abs() < 1e-6);
}

#[test]
fn upstream_label_none_when_empty() {
    assert!(upstream_label(&[]).is_none());
}

#[test]
fn upstream_label_uses_first_entry() {
    let upstreams = vec![
        UpstreamDns {
            name: "primary".into(),
            address: "1.1.1.1".into(),
            protocol: DnsProtocol::Udp,
            port: None,
        },
        UpstreamDns {
            name: "secondary".into(),
            address: "8.8.8.8".into(),
            protocol: DnsProtocol::Udp,
            port: None,
        },
    ];
    let label = upstream_label(&upstreams).expect("non-empty list returns a label");
    assert!(label.contains("1.1.1.1") || label.contains("primary"));
}

// ---------------------------------------------------------------------------
// `record_query` doesn't panic on a missing log sink, and the call signs
// don't drift.
// ---------------------------------------------------------------------------

#[test]
fn record_query_with_no_sink_is_a_noop() {
    let src = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5353);
    crate::dns::server::record_query(
        None,
        "example.com",
        RecordType::A,
        src,
        "blocked",
        None,
        Duration::from_millis(2),
    );
}
