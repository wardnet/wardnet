//! Unit tests for [`UdpDnsServer`].
//!
//! Covers the lifecycle (start / stop / running flag), config update,
//! cache flush, and the small helper functions (`record_query`,
//! `duration_to_ms`, `upstream_label`). The full hot-path through
//! `handle_query` and the stop-drains-in-flight-handlers race are
//! exercised via the e2e suite (`dns-config.spec.ts`) — the toggle path
//! there does a synchronous start→stop→start cycle that this race
//! breaks without the fix.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use chrono::Utc;
use hickory_proto::rr::RecordType;
use tokio::sync::RwLock;
use uuid::Uuid;
use wardnet_common::dns::{DnsConfig, DnsProtocol, UpstreamDns, UpstreamId};
use wardnet_common::tunnel::{Tunnel, TunnelConfig, TunnelStatus};
use wardnet_common::wireguard_config::WgPeerConfig;
use wardnetd_data::repository::TunnelRepository;
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_services::dns::server::DnsServer;

use crate::dns::server::{
    TunnelForwarderInfo, UdpDnsServer, duration_to_ms, get_or_build_tunnel_forwarder,
    upstream_label,
};
use crate::tests::stubs::StubDnsFilterService;

fn loopback_ephemeral() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
}

fn stub_filter() -> Arc<dyn wardnetd_services::DnsFilterService> {
    Arc::new(StubDnsFilterService)
}

fn empty_routing_snapshot() -> Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>> {
    Arc::new(ArcSwap::from_pointee(HashMap::new()))
}

fn stub_tunnel_repo() -> Arc<dyn TunnelRepository> {
    struct Stub;
    #[async_trait]
    impl TunnelRepository for Stub {
        async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>> {
            Ok(vec![])
        }
        async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Tunnel>> {
            Ok(None)
        }
        async fn find_config_by_id(&self, _id: &str) -> anyhow::Result<Option<TunnelConfig>> {
            Ok(None)
        }
        async fn insert(&self, _row: &TunnelRow) -> anyhow::Result<()> {
            Ok(())
        }
        async fn update_status(&self, _id: &str, _status: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn update_dns_override(&self, _id: &str, _value: bool) -> anyhow::Result<()> {
            Ok(())
        }
        async fn update_stats(
            &self,
            _id: &str,
            _bytes_tx: i64,
            _bytes_rx: i64,
            _last_handshake: Option<&str>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn delete(&self, _id: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn next_interface_index(&self) -> anyhow::Result<i64> {
            Ok(0)
        }
        async fn count(&self) -> anyhow::Result<i64> {
            Ok(0)
        }
        async fn count_active(&self) -> anyhow::Result<i64> {
            Ok(0)
        }
    }
    Arc::new(Stub)
}

/// Build a DNS server with empty routing snapshot + stub tunnel repo —
/// the lifecycle/cache tests in this file don't exercise per-tunnel
/// forwarding, so the snapshot stays empty and `find_by_id` is never
/// called. The dedicated upstream-selection tests live in their own
/// module and inject a populated snapshot directly.
fn build_test_server(config: DnsConfig, bind_addr: SocketAddr) -> UdpDnsServer {
    UdpDnsServer::with_bind_addr(
        config,
        bind_addr,
        stub_filter(),
        empty_routing_snapshot(),
        stub_tunnel_repo(),
    )
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_sets_running_flag() {
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());

    server.start().await.unwrap();
    assert!(server.is_running(), "server should be running after start");

    server.stop().await.unwrap();
}

#[tokio::test]
async fn stop_clears_running_flag() {
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());

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
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());

    server.start().await.unwrap();
    // Second start is documented as a no-op (warns + returns Ok).
    server.start().await.unwrap();
    assert!(server.is_running());

    server.stop().await.unwrap();
}

#[tokio::test]
async fn stop_when_not_running_is_a_noop() {
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());

    server.stop().await.unwrap();
    assert!(!server.is_running());
}

#[tokio::test]
async fn restart_after_stop_works() {
    // Each `start()` must create a fresh `TaskTracker`. If the tracker
    // is reused without recreation, the second `tracker.spawn(...)` after
    // a `tracker.close()` would panic. Toggling the server quickly off
    // and on (the dns-config e2e path) needs this to be safe.
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());

    server.start().await.unwrap();
    server.stop().await.unwrap();

    // The drained tracker has been replaced; start() must succeed again.
    server.start().await.unwrap();
    assert!(server.is_running());
    server.stop().await.unwrap();
}

#[tokio::test]
async fn stop_is_idempotent_after_drain() {
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());

    server.start().await.unwrap();
    server.stop().await.unwrap();
    // Second stop on an already-stopped server is documented as a no-op.
    server.stop().await.unwrap();
}

#[tokio::test]
async fn stop_drains_the_per_query_spawn() {
    // Drive a real UDP query through the loop so the server's recv branch
    // actually fires `tracker.spawn(...)`, then assert `stop()` returns
    // cleanly (the drain awaits the spawned handler regardless of how it
    // exits — cache miss → filter pass → upstream forward error in the
    // sandboxed test env). Without sending traffic, the per-query path is
    // never executed and the drain is a vacuous no-op.
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());

    server.start().await.unwrap();
    let bound = server
        .local_addr()
        .expect("server should be bound after start");

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("client bind should succeed");
    // Minimal valid DNS query: id=0x1234, RD=1, QDCOUNT=1, one A query
    // for `example.com.`. Hand-rolled to avoid pulling extra deps into
    // the test for one packet.
    let query: &[u8] = &[
        0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, b'e', b'x',
        b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    client
        .send_to(query, bound)
        .await
        .expect("send should succeed");

    // Give the server loop a chance to recv and spawn the handler before
    // we tear it down. A short sleep is enough — `stop()` then drains.
    tokio::time::sleep(Duration::from_millis(50)).await;

    server.stop().await.unwrap();
    assert!(!server.is_running());
}

#[tokio::test]
async fn concurrent_start_calls_dont_race_to_bind() {
    // Two concurrent `start()` calls used to both pass the
    // `running == false` check and both proceed to `UdpSocket::bind`,
    // racing on the same address. The loser hit EADDRINUSE. The
    // lifecycle Mutex serializes them: the first sets `running = true`,
    // the second sees it under the same lock and returns Ok with a warn.
    //
    // Reproduces the second race surfaced by the dns-config e2e
    // (`flushCache returns a count and a message`) — the API toggle
    // handler calls start() synchronously and the DnsRunner reacts to
    // the same DnsConfigChanged event in parallel.
    //
    // Probe-and-drop a UdpSocket to discover a port that's free *right
    // now*, then re-use that port for both start() calls. With the bug,
    // one of the two binds would race the other and fail. With the fix,
    // only one bind happens (the second start sees running=true under
    // the lifecycle lock and returns immediately).
    let probe = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("probe bind should succeed");
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let bind = SocketAddr::from(([127, 0, 0, 1], port));

    let server = Arc::new(build_test_server(DnsConfig::default(), bind));

    let s1 = Arc::clone(&server);
    let s2 = Arc::clone(&server);
    let (r1, r2) = tokio::join!(s1.start(), s2.start());
    r1.expect("first concurrent start should succeed");
    r2.expect("second concurrent start should be a no-op (server already running)");

    assert!(server.is_running());
    server.stop().await.unwrap();
}

#[tokio::test]
async fn flush_cache_returns_zero_on_empty() {
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());
    server.start().await.unwrap();

    let flushed = server.flush_cache().await;
    assert_eq!(flushed, 0);

    server.stop().await.unwrap();
}

#[tokio::test]
async fn update_config_works_before_and_after_start() {
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());

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
    let server = build_test_server(
        DnsConfig {
            upstream_servers: vec![],
            ..DnsConfig::default()
        },
        loopback_ephemeral(),
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

#[tokio::test]
async fn record_query_with_sink_pushes_a_row_with_normalized_domain() {
    // Trailing dots are part of DNS wire format but not what we want to
    // see in Loki / the UI. The recorder must strip them; verify both
    // that a row arrives and that the domain is normalized.
    let (sink, mut rx) = wardnetd_services::dns::log_sink::DnsLogSink::new();
    let src = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 5353);

    crate::dns::server::record_query(
        Some(sink.as_ref()),
        "example.com.",
        RecordType::AAAA,
        src,
        "passed",
        Some("1.1.1.1".into()),
        Duration::from_millis(7),
    );

    let row = rx
        .recv()
        .await
        .expect("sink should have received exactly one row");
    assert_eq!(row.domain, "example.com");
    assert_eq!(row.query_type, "AAAA");
    assert_eq!(row.result, "passed");
    assert_eq!(row.upstream.as_deref(), Some("1.1.1.1"));
    assert_eq!(row.client_ip, "127.0.0.1");
    assert!(row.device_id.is_none());
    // Latency conversion: 7ms = 7000us = 7.0
    assert!((row.latency_ms - 7.0).abs() < 1e-6);
}

// ---------------------------------------------------------------------------
// `handle_query`: cache-miss path through the running server, with the
// routing snapshot populated. The default resolver path returns SERVFAIL
// in the sandbox (Cloudflare unreachable from the runner), which is
// fine — the assertion is on the *recorded* result, not the answer.
// ---------------------------------------------------------------------------

/// Send a hand-rolled DNS query for `example.com.` (A record, id=0x1234)
/// to the given server address from a freshly bound client.
async fn fire_query(target: SocketAddr) {
    let client = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("client bind should succeed");
    let query: &[u8] = &[
        0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, b'e', b'x',
        b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    client
        .send_to(query, target)
        .await
        .expect("send should succeed");
}

#[tokio::test]
async fn server_records_query_after_handling_it() {
    // Drive a real query end-to-end and assert the log sink saw a row
    // with the expected domain — exercises the full handle_query default
    // branch (cache miss, filter pass, forward, record_query call).
    let (sink, mut rx) = wardnetd_services::dns::log_sink::DnsLogSink::new();
    let server = UdpDnsServer::with_bind_addr(
        DnsConfig::default(),
        loopback_ephemeral(),
        stub_filter(),
        empty_routing_snapshot(),
        stub_tunnel_repo(),
    )
    .with_log_sink(Arc::clone(&sink));

    server.start().await.unwrap();
    let bound = server
        .local_addr()
        .expect("server should be bound after start");

    fire_query(bound).await;

    // Wait for the recorded row, with a generous deadline — the upstream
    // forward in the sandboxed test env errors quickly and returns.
    let row = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("a row should be recorded within 2s")
        .expect("sink stays open while server is up");
    assert_eq!(row.domain, "example.com");

    server.stop().await.unwrap();
}

// ---------------------------------------------------------------------------
// `get_or_build_tunnel_forwarder` — cache + repository lookup paths.
// ---------------------------------------------------------------------------

/// Programmable tunnel repository used to drive the
/// `get_or_build_tunnel_forwarder` paths. Each test sets exactly the
/// pair of return values it needs and counts repo hits to assert the
/// in-memory cache short-circuits subsequent calls.
struct ScriptedTunnelRepo {
    tunnel: Option<Tunnel>,
    config: Option<TunnelConfig>,
    find_by_id_calls: StdMutex<u32>,
    find_config_calls: StdMutex<u32>,
}

impl ScriptedTunnelRepo {
    fn new(tunnel: Option<Tunnel>, config: Option<TunnelConfig>) -> Self {
        Self {
            tunnel,
            config,
            find_by_id_calls: StdMutex::new(0),
            find_config_calls: StdMutex::new(0),
        }
    }

    fn find_by_id_calls(&self) -> u32 {
        *self.find_by_id_calls.lock().unwrap()
    }
}

#[async_trait]
impl TunnelRepository for ScriptedTunnelRepo {
    async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>> {
        Ok(vec![])
    }
    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Tunnel>> {
        *self.find_by_id_calls.lock().unwrap() += 1;
        Ok(self.tunnel.clone())
    }
    async fn find_config_by_id(&self, _id: &str) -> anyhow::Result<Option<TunnelConfig>> {
        *self.find_config_calls.lock().unwrap() += 1;
        Ok(self.config.clone())
    }
    async fn insert(&self, _row: &TunnelRow) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_status(&self, _id: &str, _status: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_dns_override(&self, _id: &str, _value: bool) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_stats(
        &self,
        _id: &str,
        _bytes_tx: i64,
        _bytes_rx: i64,
        _last_handshake: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn next_interface_index(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn count_active(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
}

fn sample_tunnel(id: Uuid, interface: &str) -> Tunnel {
    Tunnel {
        id,
        label: "Sweden VPN".into(),
        country_code: "SE".into(),
        provider: Some("Mullvad".into()),
        interface_name: interface.into(),
        endpoint: "198.51.100.1:51820".into(),
        status: TunnelStatus::Up,
        last_handshake: None,
        bytes_tx: 0,
        bytes_rx: 0,
        created_at: Utc::now(),
        override_default_dns: true,
    }
}

fn sample_config(dns: Vec<String>) -> TunnelConfig {
    TunnelConfig {
        address: vec!["10.66.0.2/32".into()],
        dns,
        listen_port: None,
        peer: WgPeerConfig {
            public_key: "abc123".into(),
            endpoint: Some("198.51.100.1:51820".into()),
            allowed_ips: vec!["0.0.0.0/0".into()],
            preshared_key: None,
            persistent_keepalive: Some(25),
        },
        override_default_dns: true,
    }
}

fn empty_forwarder_cache() -> Arc<RwLock<HashMap<Uuid, Arc<TunnelForwarderInfo>>>> {
    Arc::new(RwLock::new(HashMap::new()))
}

#[tokio::test]
async fn get_or_build_tunnel_forwarder_returns_interface_and_upstream() {
    let id = Uuid::new_v4();
    let repo: Arc<dyn TunnelRepository> = Arc::new(ScriptedTunnelRepo::new(
        Some(sample_tunnel(id, "wg_ward0")),
        Some(sample_config(vec!["10.0.0.53".into()])),
    ));
    let cache = empty_forwarder_cache();

    let info = get_or_build_tunnel_forwarder(&cache, &repo, id)
        .await
        .unwrap();
    assert_eq!(info.interface_name, "wg_ward0");
    assert_eq!(info.upstream, "10.0.0.53:53".parse::<SocketAddr>().unwrap());

    let again = get_or_build_tunnel_forwarder(&cache, &repo, id)
        .await
        .unwrap();
    assert!(
        Arc::ptr_eq(&info, &again),
        "cache should hand back the same Arc"
    );
}

#[tokio::test]
async fn get_or_build_tunnel_forwarder_caches_after_first_miss() {
    let id = Uuid::new_v4();
    let scripted = Arc::new(ScriptedTunnelRepo::new(
        Some(sample_tunnel(id, "wg_ward1")),
        Some(sample_config(vec!["10.0.0.53".into()])),
    ));
    let repo: Arc<dyn TunnelRepository> = Arc::clone(&scripted) as _;
    let cache = empty_forwarder_cache();

    let _ = get_or_build_tunnel_forwarder(&cache, &repo, id)
        .await
        .unwrap();
    let _ = get_or_build_tunnel_forwarder(&cache, &repo, id)
        .await
        .unwrap();
    let _ = get_or_build_tunnel_forwarder(&cache, &repo, id)
        .await
        .unwrap();

    assert_eq!(
        scripted.find_by_id_calls(),
        1,
        "repo find_by_id should run exactly once across three calls"
    );
}

#[tokio::test]
async fn get_or_build_tunnel_forwarder_errors_when_tunnel_missing() {
    let repo: Arc<dyn TunnelRepository> = Arc::new(ScriptedTunnelRepo::new(None, None));
    let cache = empty_forwarder_cache();

    let err = get_or_build_tunnel_forwarder(&cache, &repo, Uuid::new_v4())
        .await
        .expect_err("missing tunnel should error");
    assert!(err.to_string().contains("not found"), "got: {err}");
}

#[tokio::test]
async fn get_or_build_tunnel_forwarder_errors_when_config_missing() {
    let id = Uuid::new_v4();
    let repo: Arc<dyn TunnelRepository> = Arc::new(ScriptedTunnelRepo::new(
        Some(sample_tunnel(id, "wg_ward0")),
        None,
    ));
    let cache = empty_forwarder_cache();

    let err = get_or_build_tunnel_forwarder(&cache, &repo, id)
        .await
        .expect_err("missing config should error");
    assert!(err.to_string().contains("no config"), "got: {err}");
}

#[tokio::test]
async fn get_or_build_tunnel_forwarder_errors_when_dns_list_empty() {
    let id = Uuid::new_v4();
    let repo: Arc<dyn TunnelRepository> = Arc::new(ScriptedTunnelRepo::new(
        Some(sample_tunnel(id, "wg_ward0")),
        Some(sample_config(vec![])),
    ));
    let cache = empty_forwarder_cache();

    let err = get_or_build_tunnel_forwarder(&cache, &repo, id)
        .await
        .expect_err("empty DNS list should error");
    assert!(err.to_string().contains("no DNS server"), "got: {err}");
}

#[tokio::test]
async fn get_or_build_tunnel_forwarder_errors_when_dns_not_an_ip() {
    let id = Uuid::new_v4();
    let repo: Arc<dyn TunnelRepository> = Arc::new(ScriptedTunnelRepo::new(
        Some(sample_tunnel(id, "wg_ward0")),
        Some(sample_config(vec!["dns.example.com".into()])),
    ));
    let cache = empty_forwarder_cache();

    let err = get_or_build_tunnel_forwarder(&cache, &repo, id)
        .await
        .expect_err("non-IP DNS entry should error");
    assert!(err.to_string().contains("not a valid IP"), "got: {err}");
}
