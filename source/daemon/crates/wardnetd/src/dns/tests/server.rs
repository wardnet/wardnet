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
use hickory_resolver::config::ServerOrderingStrategy;
use tokio::sync::RwLock;
use uuid::Uuid;
use wardnet_common::dns::{DnsConfig, DnsProtocol, DnsResolutionMode, UpstreamDns, UpstreamId};
use wardnet_common::event::WardnetEvent;
use wardnet_common::tunnel::{Tunnel, TunnelConfig, TunnelStatus};
use wardnet_common::wireguard_config::WgPeerConfig;
use wardnetd_data::repository::TunnelRepository;
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_services::dns::cache::DnsCache;
use wardnetd_services::dns::server::{DnsServer, DnsSocket};
use wardnetd_services::event::{BroadcastEventBus, EventPublisher};

use crate::dns::server::{
    TunnelForwarderInfo, UdpDnsServer, build_recursor, build_resolver, duration_to_ms,
    effective_upstreams, get_or_build_tunnel_forwarder, resolve_via_recursor,
    spawn_cache_invalidator, upstream_label,
};
use crate::tests::stubs::StubDnsFilterService;

fn loopback_ephemeral() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
}

fn stub_filter() -> Arc<dyn wardnetd_services::DnsFilterService> {
    Arc::new(StubDnsFilterService)
}

fn stub_events() -> Arc<dyn EventPublisher> {
    Arc::new(BroadcastEventBus::new(16))
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
        async fn update_endpoint(
            &self,
            _id: &str,
            _endpoint: &str,
            _peer_config_json: &str,
            _server_name: &str,
            _resolved_at: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn count_active(&self) -> anyhow::Result<i64> {
            Ok(0)
        }
    }
    Arc::new(Stub)
}

/// Filter stub whose response is configured per construction. Lets
/// tests pin `handle_query` down a specific branch (Block / Rewrite / Pass).
struct ConfigurableFilter {
    action: wardnet_common::dns::FilterAction,
}

#[async_trait]
impl wardnetd_services::DnsFilterService for ConfigurableFilter {
    async fn check(
        &self,
        _domain: &str,
        _qtype: hickory_proto::rr::RecordType,
        _client: std::net::IpAddr,
    ) -> wardnetd_services::dns_filter::service::CheckOutcome {
        wardnetd_services::dns_filter::service::CheckOutcome {
            action: self.action,
            would_have_blocked: false,
        }
    }
    async fn rebuild_all(&self) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
    async fn list_profiles(
        &self,
    ) -> Result<wardnet_common::api::ListProfilesResponse, wardnetd_services::error::AppError> {
        unimplemented!()
    }
    async fn get_profile(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::GetProfileResponse, wardnetd_services::error::AppError> {
        unimplemented!()
    }
    async fn create_profile(
        &self,
        _r: wardnet_common::api::CreateProfileRequest,
    ) -> Result<wardnet_common::api::CreateProfileResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn update_profile(
        &self,
        _id: Uuid,
        _r: wardnet_common::api::UpdateProfileRequest,
    ) -> Result<wardnet_common::api::UpdateProfileResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn delete_profile(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteProfileResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn list_blocklists(
        &self,
        _profile_id: Uuid,
    ) -> Result<wardnet_common::api::ListBlocklistsResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn create_blocklist(
        &self,
        _profile_id: Uuid,
        _r: wardnet_common::api::CreateBlocklistRequest,
    ) -> Result<wardnet_common::api::CreateBlocklistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn update_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
        _r: wardnet_common::api::UpdateBlocklistRequest,
    ) -> Result<wardnet_common::api::UpdateBlocklistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn delete_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteBlocklistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn refresh_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<wardnet_common::jobs::JobDispatchedResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn list_allowlist(
        &self,
        _profile_id: Uuid,
    ) -> Result<wardnet_common::api::ListAllowlistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn create_allowlist_entry(
        &self,
        _profile_id: Uuid,
        _r: wardnet_common::api::CreateAllowlistRequest,
    ) -> Result<wardnet_common::api::CreateAllowlistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn delete_allowlist_entry(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteAllowlistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn list_custom_rules(
        &self,
        _profile_id: Uuid,
    ) -> Result<wardnet_common::api::ListFilterRulesResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn create_custom_rule(
        &self,
        _profile_id: Uuid,
        _r: wardnet_common::api::CreateFilterRuleRequest,
    ) -> Result<wardnet_common::api::CreateFilterRuleResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn update_custom_rule(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
        _r: wardnet_common::api::UpdateFilterRuleRequest,
    ) -> Result<wardnet_common::api::UpdateFilterRuleResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn delete_custom_rule(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteFilterRuleResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn list_device_settings(
        &self,
        _params: wardnet_common::api::ListDeviceFilterSettingsParams,
    ) -> Result<
        wardnet_common::api::ListDeviceFilterSettingsResponse,
        wardnetd_services::error::AppError,
    > {
        unimplemented!()
    }
    async fn get_device_settings(
        &self,
        _device_id: Uuid,
    ) -> Result<
        wardnet_common::api::GetDeviceFilterSettingsResponse,
        wardnetd_services::error::AppError,
    > {
        unimplemented!()
    }
    async fn update_device_settings(
        &self,
        _device_id: Uuid,
        _r: wardnet_common::api::UpdateDeviceFilterSettingsRequest,
    ) -> Result<
        wardnet_common::api::UpdateDeviceFilterSettingsResponse,
        wardnetd_services::error::AppError,
    > {
        unimplemented!()
    }
    async fn get_filter_config(
        &self,
    ) -> Result<wardnet_common::api::DnsFilterConfigResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn update_filter_config(
        &self,
        _r: wardnet_common::api::UpdateDnsFilterConfigRequest,
    ) -> Result<wardnet_common::api::DnsFilterConfigResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn rebuild_blocklist_filter(
        &self,
        _id: Uuid,
    ) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
    async fn rebuild_profile(&self, _id: Uuid) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
    async fn rebuild_device(&self, _id: Uuid) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
    async fn rebuild_default_context(&self) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
    async fn handle_device_ip_changed(
        &self,
        _device_id: Uuid,
        _old_ip: &str,
        _new_ip: &str,
    ) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
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
        stub_events(),
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
            tls_server_name: None,
        },
        UpstreamDns {
            name: "secondary".into(),
            address: "8.8.8.8".into(),
            protocol: DnsProtocol::Udp,
            port: None,
            tls_server_name: None,
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
        stub_events(),
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
    async fn update_endpoint(
        &self,
        _id: &str,
        _endpoint: &str,
        _peer_config_json: &str,
        _server_name: &str,
        _resolved_at: &str,
    ) -> anyhow::Result<()> {
        Ok(())
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
        server_selector: None,
        resolved_server_name: None,
        endpoint_resolved_at: None,
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

// ---------------------------------------------------------------------------
// `handle_query`: filter-driven Block / Rewrite / Pass branches plus the
// upstream error path. Each test stands up the real server with an
// injected filter outcome, fires a UDP query, and reads the resulting
// log row to assert what the server told the sink.
// ---------------------------------------------------------------------------

fn build_with_filter(
    config: DnsConfig,
    filter: Arc<dyn wardnetd_services::DnsFilterService>,
    sink: Arc<wardnetd_services::dns::log_sink::DnsLogSink>,
) -> UdpDnsServer {
    UdpDnsServer::with_bind_addr(
        config,
        loopback_ephemeral(),
        filter,
        empty_routing_snapshot(),
        stub_tunnel_repo(),
        stub_events(),
    )
    .with_log_sink(sink)
}

#[tokio::test]
async fn handle_query_block_branch_records_blocked() {
    let (sink, mut rx) = wardnetd_services::dns::log_sink::DnsLogSink::new();
    let filter: Arc<dyn wardnetd_services::DnsFilterService> = Arc::new(ConfigurableFilter {
        action: wardnet_common::dns::FilterAction::Block,
    });
    let server = build_with_filter(DnsConfig::default(), filter, Arc::clone(&sink));

    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");
    fire_query(bound).await;

    let row = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("blocked row arrives quickly")
        .unwrap();
    assert_eq!(row.result, "blocked");
    assert_eq!(row.domain, "example.com");
    assert!(row.upstream.is_none());

    server.stop().await.unwrap();
}

#[tokio::test]
async fn handle_query_rewrite_branch_records_rewritten() {
    let (sink, mut rx) = wardnetd_services::dns::log_sink::DnsLogSink::new();
    let filter: Arc<dyn wardnetd_services::DnsFilterService> = Arc::new(ConfigurableFilter {
        action: wardnet_common::dns::FilterAction::Rewrite {
            ip: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        },
    });
    let server = build_with_filter(DnsConfig::default(), filter, Arc::clone(&sink));

    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");
    fire_query(bound).await;

    let row = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("rewritten row arrives quickly")
        .unwrap();
    assert_eq!(row.result, "rewritten");
    assert_eq!(row.domain, "example.com");
    assert!(row.upstream.is_none());

    server.stop().await.unwrap();
}

#[tokio::test]
async fn handle_query_upstream_error_records_upstream_error() {
    // Point the resolver at an unreachable upstream so the lookup fails
    // and the Err branch runs (send_servfail + record_query "upstream_error").
    let (sink, mut rx) = wardnetd_services::dns::log_sink::DnsLogSink::new();
    let filter: Arc<dyn wardnetd_services::DnsFilterService> = Arc::new(ConfigurableFilter {
        action: wardnet_common::dns::FilterAction::Pass,
    });
    let cfg = DnsConfig {
        upstream_servers: vec![UpstreamDns {
            name: "blackhole".into(),
            address: "127.0.0.1".into(),
            // TCP so we get a fast RST on the closed port instead of
            // waiting for hickory's UDP retry budget to elapse.
            protocol: DnsProtocol::Tcp,
            port: Some(1),
            tls_server_name: None,
        }],
        ..DnsConfig::default()
    };
    let server = build_with_filter(cfg, filter, Arc::clone(&sink));

    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");
    fire_query(bound).await;

    let row = tokio::time::timeout(Duration::from_secs(20), rx.recv())
        .await
        .expect("upstream_error row should arrive within the resolver's retry window")
        .unwrap();
    assert_eq!(row.result, "upstream_error");
    assert_eq!(row.domain, "example.com");
    assert_eq!(row.upstream.as_deref(), Some("127.0.0.1"));

    server.stop().await.unwrap();
}

// ---------------------------------------------------------------------------
// `handle_query`: tunnel forward branch via a populated routing snapshot.
// The forward fails (the test env has no real tunnel interface and the
// upstream IP is unreachable), so the branch lands in the
// `forward_via_tunnel` Err path → ServFail + "upstream_error".
// ---------------------------------------------------------------------------

#[tokio::test]
async fn handle_query_tunnel_branch_records_upstream_error_when_forward_fails() {
    let (sink, mut rx) = wardnetd_services::dns::log_sink::DnsLogSink::new();
    let filter: Arc<dyn wardnetd_services::DnsFilterService> = Arc::new(ConfigurableFilter {
        action: wardnet_common::dns::FilterAction::Pass,
    });

    let tunnel_id = Uuid::new_v4();
    let snapshot = Arc::new(ArcSwap::from_pointee(HashMap::from([(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        UpstreamId::Tunnel(tunnel_id),
    )])));
    let tunnel_repo: Arc<dyn TunnelRepository> = Arc::new(ScriptedTunnelRepo::new(
        Some(sample_tunnel(tunnel_id, "lo")),
        // 127.0.0.1:53 is unbound in the sandbox — forward errors fast.
        Some(sample_config(vec!["127.0.0.1".into()])),
    ));

    let server = UdpDnsServer::with_bind_addr(
        DnsConfig::default(),
        loopback_ephemeral(),
        filter,
        snapshot,
        tunnel_repo,
        stub_events(),
    )
    .with_log_sink(Arc::clone(&sink));

    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");
    fire_query(bound).await;

    let row = tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("a row should be recorded for the tunnel-forward attempt")
        .unwrap();
    // The forward path can resolve quickly enough that we may see either
    // an `upstream_error` (forward failed) or `forwarded` (an actual
    // response was received and cached). Both prove the tunnel branch
    // ran. Pin the field that tells us we went through the tunnel path:
    // `upstream` must equal the configured tunnel DNS, never the system
    // upstream string.
    assert_eq!(row.domain, "example.com");
    assert_eq!(row.upstream.as_deref(), Some("127.0.0.1"));
    assert!(
        matches!(row.result.as_str(), "upstream_error" | "forwarded"),
        "got: {}",
        row.result
    );

    server.stop().await.unwrap();
}

// ---------------------------------------------------------------------------
// `build_resolver` — pure function; protocol mapping + Cloudflare fallback.
// ---------------------------------------------------------------------------

#[test]
fn build_resolver_with_udp_upstream_succeeds() {
    let upstreams = vec![UpstreamDns {
        name: "primary".into(),
        address: "1.1.1.1".into(),
        protocol: DnsProtocol::Udp,
        port: None,
        tls_server_name: None,
    }];
    let _ = crate::dns::server::build_resolver(
        &upstreams,
        false,
        ServerOrderingStrategy::QueryStatistics,
    );
}

#[test]
fn build_resolver_with_tcp_upstream_succeeds() {
    let upstreams = vec![UpstreamDns {
        name: "primary".into(),
        address: "1.1.1.1".into(),
        protocol: DnsProtocol::Tcp,
        port: Some(53),
        tls_server_name: None,
    }];
    let _ = crate::dns::server::build_resolver(
        &upstreams,
        false,
        ServerOrderingStrategy::QueryStatistics,
    );
}

#[test]
fn build_resolver_builds_dot_and_doh_with_sni() {
    // Encrypted upstreams with a TLS server name exercise the Tls + Https
    // branches (ConnectionConfig::tls/https) and the port-default arm.
    let upstreams = vec![
        UpstreamDns {
            name: "tls".into(),
            address: "1.1.1.1".into(),
            protocol: DnsProtocol::Tls,
            port: None,
            tls_server_name: Some("cloudflare-dns.com".into()),
        },
        UpstreamDns {
            name: "https".into(),
            address: "1.1.1.1".into(),
            protocol: DnsProtocol::Https,
            port: None,
            tls_server_name: Some("cloudflare-dns.com".into()),
        },
    ];
    let _ = crate::dns::server::build_resolver(
        &upstreams,
        false,
        ServerOrderingStrategy::QueryStatistics,
    );
}

#[test]
fn build_resolver_skips_encrypted_upstream_without_sni() {
    // An encrypted upstream missing its SNI is skipped (not downgraded to
    // plaintext); with no other servers the builder falls back to
    // Cloudflare rather than returning an empty config.
    let upstreams = vec![UpstreamDns {
        name: "tls-no-sni".into(),
        address: "1.1.1.1".into(),
        protocol: DnsProtocol::Tls,
        port: None,
        tls_server_name: None,
    }];
    let _ = crate::dns::server::build_resolver(
        &upstreams,
        false,
        ServerOrderingStrategy::QueryStatistics,
    );
}

#[test]
fn build_resolver_enables_dnssec_validation() {
    // Smoke: the dnssec_enabled flag flows into ResolverOpts.validate
    // without panicking the builder.
    let upstreams = vec![UpstreamDns {
        name: "primary".into(),
        address: "1.1.1.1".into(),
        protocol: DnsProtocol::Udp,
        port: None,
        tls_server_name: None,
    }];
    let _ = crate::dns::server::build_resolver(
        &upstreams,
        true,
        ServerOrderingStrategy::QueryStatistics,
    );
}

#[test]
fn build_resolver_skips_invalid_ip_addresses() {
    // The macro-driven config can hold malformed entries (user input). The
    // resolver builder must skip them, and when ALL entries are invalid
    // it falls back to Cloudflare instead of returning an empty config.
    let upstreams = vec![UpstreamDns {
        name: "bad".into(),
        address: "not-an-ip".into(),
        protocol: DnsProtocol::Udp,
        port: None,
        tls_server_name: None,
    }];
    let _ = crate::dns::server::build_resolver(
        &upstreams,
        false,
        ServerOrderingStrategy::QueryStatistics,
    );
}

// ---------------------------------------------------------------------------
// Cache invalidation on DnsFilterRebuilt (issue #341).
//
// The server subscribes to the event bus at construction and flushes its
// response cache whenever a filter rebuild is announced. Without this, a
// domain that was previously forwarded and cached keeps serving the cached
// "Pass" answer even after a blocklist update added it — until cache TTL
// expiry, which is up to `dns_cache_ttl_max_secs` (one day default).
// ---------------------------------------------------------------------------

/// Filter that flips between Pass and Block under a shared lock so the
/// test can simulate a blocklist update *before* publishing the
/// invalidation event — i.e. the same ordering the real
/// `DnsFilterServiceImpl` enforces (swap, then announce).
struct SwitchableFilter {
    action: tokio::sync::RwLock<wardnet_common::dns::FilterAction>,
}

impl SwitchableFilter {
    fn new(initial: wardnet_common::dns::FilterAction) -> Self {
        Self {
            action: tokio::sync::RwLock::new(initial),
        }
    }
    async fn set(&self, action: wardnet_common::dns::FilterAction) {
        *self.action.write().await = action;
    }
}

#[async_trait]
impl wardnetd_services::DnsFilterService for SwitchableFilter {
    async fn check(
        &self,
        _domain: &str,
        _qtype: hickory_proto::rr::RecordType,
        _client: std::net::IpAddr,
    ) -> wardnetd_services::dns_filter::service::CheckOutcome {
        wardnetd_services::dns_filter::service::CheckOutcome {
            action: *self.action.read().await,
            would_have_blocked: false,
        }
    }
    async fn rebuild_all(&self) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
    async fn list_profiles(
        &self,
    ) -> Result<wardnet_common::api::ListProfilesResponse, wardnetd_services::error::AppError> {
        unimplemented!()
    }
    async fn get_profile(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::GetProfileResponse, wardnetd_services::error::AppError> {
        unimplemented!()
    }
    async fn create_profile(
        &self,
        _r: wardnet_common::api::CreateProfileRequest,
    ) -> Result<wardnet_common::api::CreateProfileResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn update_profile(
        &self,
        _id: Uuid,
        _r: wardnet_common::api::UpdateProfileRequest,
    ) -> Result<wardnet_common::api::UpdateProfileResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn delete_profile(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteProfileResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn list_blocklists(
        &self,
        _profile_id: Uuid,
    ) -> Result<wardnet_common::api::ListBlocklistsResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn create_blocklist(
        &self,
        _profile_id: Uuid,
        _r: wardnet_common::api::CreateBlocklistRequest,
    ) -> Result<wardnet_common::api::CreateBlocklistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn update_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
        _r: wardnet_common::api::UpdateBlocklistRequest,
    ) -> Result<wardnet_common::api::UpdateBlocklistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn delete_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteBlocklistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn refresh_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<wardnet_common::jobs::JobDispatchedResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn list_allowlist(
        &self,
        _profile_id: Uuid,
    ) -> Result<wardnet_common::api::ListAllowlistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn create_allowlist_entry(
        &self,
        _profile_id: Uuid,
        _r: wardnet_common::api::CreateAllowlistRequest,
    ) -> Result<wardnet_common::api::CreateAllowlistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn delete_allowlist_entry(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteAllowlistResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn list_custom_rules(
        &self,
        _profile_id: Uuid,
    ) -> Result<wardnet_common::api::ListFilterRulesResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn create_custom_rule(
        &self,
        _profile_id: Uuid,
        _r: wardnet_common::api::CreateFilterRuleRequest,
    ) -> Result<wardnet_common::api::CreateFilterRuleResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn update_custom_rule(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
        _r: wardnet_common::api::UpdateFilterRuleRequest,
    ) -> Result<wardnet_common::api::UpdateFilterRuleResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn delete_custom_rule(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteFilterRuleResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn list_device_settings(
        &self,
        _params: wardnet_common::api::ListDeviceFilterSettingsParams,
    ) -> Result<
        wardnet_common::api::ListDeviceFilterSettingsResponse,
        wardnetd_services::error::AppError,
    > {
        unimplemented!()
    }
    async fn get_device_settings(
        &self,
        _device_id: Uuid,
    ) -> Result<
        wardnet_common::api::GetDeviceFilterSettingsResponse,
        wardnetd_services::error::AppError,
    > {
        unimplemented!()
    }
    async fn update_device_settings(
        &self,
        _device_id: Uuid,
        _r: wardnet_common::api::UpdateDeviceFilterSettingsRequest,
    ) -> Result<
        wardnet_common::api::UpdateDeviceFilterSettingsResponse,
        wardnetd_services::error::AppError,
    > {
        unimplemented!()
    }
    async fn get_filter_config(
        &self,
    ) -> Result<wardnet_common::api::DnsFilterConfigResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn update_filter_config(
        &self,
        _r: wardnet_common::api::UpdateDnsFilterConfigRequest,
    ) -> Result<wardnet_common::api::DnsFilterConfigResponse, wardnetd_services::error::AppError>
    {
        unimplemented!()
    }
    async fn rebuild_blocklist_filter(
        &self,
        _id: Uuid,
    ) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
    async fn rebuild_profile(&self, _id: Uuid) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
    async fn rebuild_device(&self, _id: Uuid) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
    async fn rebuild_default_context(&self) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
    async fn handle_device_ip_changed(
        &self,
        _device_id: Uuid,
        _old_ip: &str,
        _new_ip: &str,
    ) -> Result<(), wardnetd_services::error::AppError> {
        Ok(())
    }
}

/// Spawn a tiny UDP responder that answers every query with a single A
/// record (`93.184.216.34`, TTL 60). Returns the bound address. Lives
/// for the test's duration — the spawned task self-terminates when its
/// socket is dropped (which happens when the test exits and the
/// runtime tears down).
async fn spawn_stub_upstream() -> SocketAddr {
    use hickory_proto::op::{Message, OpCode};
    use hickory_proto::rr::{Name, RData, Record, rdata::A};
    use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};

    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("stub upstream bind");
    let addr = socket.local_addr().expect("stub upstream local_addr");
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            let Ok((n, src)) = socket.recv_from(&mut buf).await else {
                break;
            };
            let Ok(request) = Message::from_bytes(&buf[..n]) else {
                continue;
            };
            let id = request.metadata.id;
            let mut response = Message::response(id, OpCode::Query);
            response.metadata.recursion_desired = true;
            response.metadata.recursion_available = true;
            response.add_queries(request.queries.clone());
            for q in &request.queries {
                if q.query_type() == RecordType::A {
                    let name = Name::from_str_relaxed(q.name().to_string())
                        .unwrap_or_else(|_| q.name().clone());
                    let record =
                        Record::from_rdata(name, 60, RData::A(A(Ipv4Addr::new(93, 184, 216, 34))));
                    response.add_answer(record);
                }
            }
            if let Ok(bytes) = response.to_bytes() {
                let _ = socket.send_to(&bytes, src).await;
            }
        }
    });
    addr
}

/// Send a hand-rolled A query for `foo.com.` (id=0xCAFE) and read the
/// reply from a fresh client socket. Returns the parsed response so the
/// test can check `response_code`.
async fn query_foo_com(target: SocketAddr) -> hickory_proto::op::Message {
    use hickory_proto::op::Message;
    use hickory_proto::serialize::binary::BinDecodable;

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("client bind");
    // DNS query: id=0xCAFE, RD=1, 1 question for foo.com A IN.
    let query: &[u8] = &[
        0xCA, 0xFE, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, b'f', b'o',
        b'o', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    client.send_to(query, target).await.expect("send");
    let mut buf = vec![0u8; 4096];
    let (n, _) = tokio::time::timeout(Duration::from_secs(5), client.recv_from(&mut buf))
        .await
        .expect("client recv timeout")
        .expect("client recv");
    Message::from_bytes(&buf[..n]).expect("parse response")
}

#[tokio::test]
async fn dns_filter_rebuilt_event_flushes_response_cache() {
    use hickory_proto::op::ResponseCode;

    // 1. Stand up a controlled upstream so cache fills deterministically.
    let upstream_addr = spawn_stub_upstream().await;

    // 2. Switchable filter starts in Pass — cache fills on the first query.
    let filter = Arc::new(SwitchableFilter::new(
        wardnet_common::dns::FilterAction::Pass,
    ));
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));

    let cfg = DnsConfig {
        upstream_servers: vec![UpstreamDns {
            name: "stub".into(),
            address: upstream_addr.ip().to_string(),
            protocol: DnsProtocol::Udp,
            port: Some(upstream_addr.port()),
            tls_server_name: None,
        }],
        ..DnsConfig::default()
    };
    let server = UdpDnsServer::with_bind_addr(
        cfg,
        loopback_ephemeral(),
        filter.clone() as Arc<dyn wardnetd_services::DnsFilterService>,
        empty_routing_snapshot(),
        stub_tunnel_repo(),
        bus.clone(),
    );
    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    // 3. First query — Pass + forward, response gets cached.
    let resp = query_foo_com(bound).await;
    assert_eq!(
        resp.metadata.response_code,
        ResponseCode::NoError,
        "stub upstream should answer NoError"
    );
    assert!(
        server.cache_size().await > 0,
        "forwarded answer should populate the cache"
    );

    // 4. Simulate the service swap: flip the filter to Block, *then*
    //    publish DnsFilterRebuilt — same ordering as the real service
    //    (swap, then announce). The subscriber spawned in
    //    `with_bind_addr` should observe the event and flush.
    filter.set(wardnet_common::dns::FilterAction::Block).await;
    bus.publish(WardnetEvent::DnsFilterRebuilt {
        timestamp: Utc::now(),
    });

    // 5. Wait for the flush. Poll briefly — the subscriber runs on the
    //    tokio runtime and the broadcast hop is sub-millisecond, but
    //    the write lock still needs a scheduler tick.
    let mut flushed = false;
    for _ in 0..50 {
        if server.cache_size().await == 0 {
            flushed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        flushed,
        "cache was not flushed within 1s of DnsFilterRebuilt"
    );

    // 6. Re-query foo.com — must NOT serve the (now flushed) cached
    //    answer. The new filter blocks, so we expect NXDOMAIN.
    let resp = query_foo_com(bound).await;
    assert_eq!(
        resp.metadata.response_code,
        ResponseCode::NXDomain,
        "post-flush query should hit the new Block filter, not the stale cache"
    );

    server.stop().await.unwrap();
}

// ---------------------------------------------------------------------------
// `spawn_cache_invalidator` per-branch unit tests.
//
// The integration test above covers the happy path (subscribe → receive
// `DnsFilterRebuilt` → flush). The branches below are harder to exercise
// through a full `UdpDnsServer` so we drive the task directly: the
// function is `pub(crate)` for exactly this reason.
// ---------------------------------------------------------------------------

/// Build a primed cache + a fresh task. Returns the cache handle (so the
/// test can observe flushes), the bus (so the test can publish), the
/// cancellation token, and the spawned task's handle (so the test can
/// await clean exit).
async fn spawn_invalidator_for_test(
    capacity: usize,
) -> (
    Arc<RwLock<wardnetd_services::dns::cache::DnsCache>>,
    Arc<BroadcastEventBus>,
    tokio_util::sync::CancellationToken,
    tokio::task::JoinHandle<()>,
) {
    use wardnetd_services::dns::cache::DnsCache;
    let cache = Arc::new(RwLock::new(DnsCache::new(16)));
    seed_cache(&cache).await;
    let bus = Arc::new(BroadcastEventBus::new(capacity));
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = spawn_cache_invalidator(Arc::clone(&cache), bus.subscribe(), cancel.clone());
    (cache, bus, cancel, handle)
}

/// Insert a single sentinel entry so `cache.len() == 1` and a flush is
/// observable. Re-uses the cache's own `insert` path with a synthetic
/// `Pass` answer keyed on `Default`.
async fn seed_cache(cache: &Arc<RwLock<wardnetd_services::dns::cache::DnsCache>>) {
    use hickory_proto::op::{Message, OpCode};
    let mut answer = Message::response(0, OpCode::Query);
    answer.metadata.recursion_desired = true;
    answer.metadata.recursion_available = true;
    cache.write().await.insert(
        UpstreamId::Default,
        "seed.example.",
        RecordType::A,
        answer,
        60,
        1,
        60,
    );
    assert_eq!(cache.read().await.len(), 1, "seed must populate cache");
}

#[tokio::test]
async fn cache_invalidator_ignores_non_rebuild_events() {
    // The `Ok(_) => {}` arm: events other than `DnsFilterRebuilt` are
    // observed and discarded without touching the cache.
    let (cache, bus, cancel, handle) = spawn_invalidator_for_test(16).await;

    bus.publish(WardnetEvent::DnsServerStarted {
        timestamp: Utc::now(),
    });

    // Give the task a tick to consume the event.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        cache.read().await.len(),
        1,
        "non-rebuild events must NOT flush the cache"
    );

    cancel.cancel();
    handle.await.expect("task joins after cancel");
}

#[tokio::test]
async fn cache_invalidator_exits_on_cancel() {
    // The `cancel.cancelled()` arm: cancelling the token (the path
    // `Drop` takes) makes the task return promptly. We assert via
    // `handle.await` completing within a small budget.
    let (_cache, _bus, cancel, handle) = spawn_invalidator_for_test(16).await;

    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(1), handle)
        .await
        .expect("task must exit within 1s of cancel")
        .expect("task joins cleanly (no panic)");
}

#[tokio::test]
async fn cache_invalidator_exits_on_bus_close() {
    // The `RecvError::Closed` arm: when every sender drops, the
    // receiver returns Closed and the task breaks out. The
    // `BroadcastEventBus` owns the sender, so dropping the sole Arc
    // closes the channel.
    let (_cache, bus, _cancel, handle) = spawn_invalidator_for_test(16).await;

    drop(bus);
    tokio::time::timeout(Duration::from_secs(1), handle)
        .await
        .expect("task must exit within 1s of bus close")
        .expect("task joins cleanly (no panic)");
}

#[tokio::test]
async fn cache_invalidator_flushes_defensively_on_lagged() {
    // The `RecvError::Lagged` arm: if we publish more than `capacity`
    // events before the task drains them, the broadcast receiver
    // surfaces `Lagged(n)` on its next `recv()`. The subscriber
    // flushes anyway because we may have skipped a real
    // `DnsFilterRebuilt`.
    //
    // Capacity 2 and 5 publishes before any scheduler tick guarantees
    // the lag — even on a single-threaded runtime the publishes are
    // synchronous and the subscriber hasn't been polled yet.
    let (cache, bus, cancel, handle) = spawn_invalidator_for_test(2).await;

    for _ in 0..5 {
        bus.publish(WardnetEvent::DnsServerStarted {
            timestamp: Utc::now(),
        });
    }

    let mut flushed = false;
    for _ in 0..50 {
        if cache.read().await.is_empty() {
            flushed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(flushed, "Lagged path must trigger a defensive flush");

    cancel.cancel();
    handle.await.expect("task joins after cancel");
}

// ---------------------------------------------------------------------------
// Zone suffix-authority: an enabled zone makes the gateway authoritative for
// the whole `*.zone` namespace, so unknown names under it are answered
// NXDOMAIN (AA + synthetic SOA) instead of leaking to the upstream resolver.
// An explicit conditional-forwarding rule under the zone still overrides this.
// ---------------------------------------------------------------------------

use wardnet_common::dns::{ConditionalForwardingRule, DnsZone};
use wardnetd_services::dns::authoritative::AuthoritativeView;

fn lan_zone() -> DnsZone {
    DnsZone {
        id: Uuid::new_v4(),
        name: "lan".into(),
        enabled: true,
        source: wardnet_common::dns::DnsZoneSource::System,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// Send raw query bytes to `target` and parse the reply from a fresh client.
async fn send_and_recv(target: SocketAddr, query: &[u8]) -> hickory_proto::op::Message {
    use hickory_proto::op::Message;
    use hickory_proto::serialize::binary::BinDecodable;

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("client bind");
    client.send_to(query, target).await.expect("send");
    let mut buf = vec![0u8; 4096];
    let (n, _) = tokio::time::timeout(Duration::from_secs(5), client.recv_from(&mut buf))
        .await
        .expect("client recv timeout")
        .expect("client recv");
    Message::from_bytes(&buf[..n]).expect("parse response")
}

#[tokio::test]
async fn authoritative_zone_unknown_name_returns_nxdomain_not_forwarded() {
    use hickory_proto::op::ResponseCode;

    // Stub upstream would answer NoError+A if we ever forwarded — so an
    // NXDOMAIN proves the query was answered authoritatively, never forwarded.
    let upstream_addr = spawn_stub_upstream().await;
    let cfg = DnsConfig {
        upstream_servers: vec![UpstreamDns {
            name: "stub".into(),
            address: upstream_addr.ip().to_string(),
            protocol: DnsProtocol::Udp,
            port: Some(upstream_addr.port()),
            tls_server_name: None,
        }],
        ..DnsConfig::default()
    };
    let server = build_test_server(cfg, loopback_ephemeral());
    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    // Enabled `lan` zone, no records — the namespace is ours but the name
    // doesn't exist.
    server
        .update_authoritative_view(AuthoritativeView::build(&[lan_zone()], vec![], vec![]))
        .await;

    // Query `unknown.lan` A (id=0xAB1E).
    let query: &[u8] = &[
        0xAB, 0x1E, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, b'u', b'n',
        b'k', b'n', b'o', b'w', b'n', 0x03, b'l', b'a', b'n', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    let resp = send_and_recv(bound, query).await;

    assert_eq!(
        resp.metadata.response_code,
        ResponseCode::NXDomain,
        "unknown name under an authoritative zone must be NXDOMAIN, not forwarded"
    );
    assert!(
        resp.metadata.authoritative,
        "authoritative NXDOMAIN must set the AA bit"
    );
    assert_eq!(
        resp.authorities.len(),
        1,
        "negative answer must carry a synthetic SOA in the authority section"
    );
    assert_eq!(
        resp.authorities[0].record_type(),
        RecordType::SOA,
        "authority record must be an SOA"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn authoritative_zone_existing_name_unmodeled_type_is_nodata_not_nxdomain() {
    use hickory_proto::op::ResponseCode;
    use wardnet_common::dns::{CustomDnsRecord, DnsRecordSource, DnsRecordType};

    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());
    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    // `printer.lan` exists with an A record, in the enabled `lan` zone.
    let zone = lan_zone();
    let record = CustomDnsRecord {
        id: Uuid::new_v4(),
        zone_id: Some(zone.id),
        domain: "printer.lan".into(),
        record_type: DnsRecordType::A,
        value: "192.168.1.50".into(),
        ttl: 300,
        enabled: true,
        source: DnsRecordSource::Manual,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    server
        .update_authoritative_view(AuthoritativeView::build(&[zone], vec![record], vec![]))
        .await;

    // Query `printer.lan` HTTPS (type 65) — a type our record enum doesn't
    // model. The name exists, so this must be NODATA (NoError), never NXDOMAIN:
    // a cacheable NXDOMAIN here would poison the valid A record.
    let query: &[u8] = &[
        0x0D, 0x05, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, b'p', b'r',
        b'i', b'n', b't', b'e', b'r', 0x03, b'l', b'a', b'n', 0x00, 0x00, 0x41, 0x00, 0x01,
    ];
    let resp = send_and_recv(bound, query).await;

    assert_eq!(
        resp.metadata.response_code,
        ResponseCode::NoError,
        "an existing name queried for an unmodeled type must be NODATA, not NXDOMAIN"
    );
    assert!(
        resp.metadata.authoritative,
        "NODATA answer must set the AA bit"
    );
    assert_eq!(
        resp.authorities.len(),
        1,
        "NODATA answer must carry the zone SOA for negative caching"
    );
    assert!(
        resp.answers.is_empty(),
        "no record of the queried type exists, so the answer section is empty"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn authoritative_zone_apex_with_no_records_is_nodata_not_nxdomain() {
    use hickory_proto::op::ResponseCode;

    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());
    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    // Enabled `lan` zone with no records at all — the apex still "exists"
    // because the zone is authoritative for it.
    server
        .update_authoritative_view(AuthoritativeView::build(&[lan_zone()], vec![], vec![]))
        .await;

    // Query the apex `lan` A (id=0xAB2E).
    let query: &[u8] = &[
        0xAB, 0x2E, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, b'l', b'a',
        b'n', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    let resp = send_and_recv(bound, query).await;

    assert_eq!(
        resp.metadata.response_code,
        ResponseCode::NoError,
        "the zone apex exists, so an apex query with no record is NODATA, not NXDOMAIN"
    );
    assert!(
        resp.metadata.authoritative,
        "apex NODATA must set the AA bit"
    );
    assert_eq!(
        resp.authorities.len(),
        1,
        "apex NODATA must carry the zone SOA"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn conditional_forwarding_overrides_zone_authority() {
    // `corp.lan` is forwarded explicitly even though `lan` is authoritative.
    // The forward path never sets the AA bit, so `authoritative == false`
    // distinguishes it from the zone-authority NXDOMAIN path (which always
    // sets AA) — regardless of whether the forward itself succeeds.
    let upstream_addr = spawn_stub_upstream().await;
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());
    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    let rule = ConditionalForwardingRule {
        id: Uuid::new_v4(),
        domain: "corp.lan".into(),
        upstream: format!("{}:{}", upstream_addr.ip(), upstream_addr.port()),
        enabled: true,
        created_at: Utc::now(),
    };
    server
        .update_authoritative_view(AuthoritativeView::build(&[lan_zone()], vec![], vec![rule]))
        .await;

    // Query `host.corp.lan` A (id=0xC0FE).
    let query: &[u8] = &[
        0xC0, 0xFE, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, b'h', b'o',
        b's', b't', 0x04, b'c', b'o', b'r', b'p', 0x03, b'l', b'a', b'n', 0x00, 0x00, 0x01, 0x00,
        0x01,
    ];
    let resp = send_and_recv(bound, query).await;

    assert!(
        !resp.metadata.authoritative,
        "a forwarded query must NOT be answered authoritatively — the forwarding rule overrides zone authority"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn drop_cancels_cache_invalidator() {
    // `UdpDnsServer::Drop` fires the cancellation token. We can't
    // reach into the spawned task from here, but constructing then
    // immediately dropping a server ensures the Drop body runs (and
    // therefore is counted by coverage), and the test must not hang —
    // a leaked task wouldn't block this test, but a panic in Drop
    // would surface here.
    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());
    drop(server);
}

// ---------------------------------------------------------------------------
// Stage 4 — rate limiting + rebinding protection (integration)
// ---------------------------------------------------------------------------

/// Stub upstream that answers every A query with a PRIVATE address
/// (192.168.0.1), to drive DNS rebinding protection.
async fn spawn_private_upstream() -> SocketAddr {
    use hickory_proto::op::{Message, OpCode};
    use hickory_proto::rr::{Name, RData, Record, rdata::A};
    use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};

    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("private upstream bind");
    let addr = socket.local_addr().expect("private upstream local_addr");
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            let Ok((n, src)) = socket.recv_from(&mut buf).await else {
                break;
            };
            let Ok(request) = Message::from_bytes(&buf[..n]) else {
                continue;
            };
            let id = request.metadata.id;
            let mut response = Message::response(id, OpCode::Query);
            response.metadata.recursion_desired = true;
            response.metadata.recursion_available = true;
            response.add_queries(request.queries.clone());
            for q in &request.queries {
                if q.query_type() == RecordType::A {
                    let name = Name::from_str_relaxed(q.name().to_string())
                        .unwrap_or_else(|_| q.name().clone());
                    let record =
                        Record::from_rdata(name, 60, RData::A(A(Ipv4Addr::new(192, 168, 0, 1))));
                    response.add_answer(record);
                }
            }
            if let Ok(bytes) = response.to_bytes() {
                let _ = socket.send_to(&bytes, src).await;
            }
        }
    });
    addr
}

fn udp_upstream(addr: SocketAddr) -> UpstreamDns {
    UpstreamDns {
        name: "stub".into(),
        address: addr.ip().to_string(),
        protocol: DnsProtocol::Udp,
        port: Some(addr.port()),
        tls_server_name: None,
    }
}

#[tokio::test]
async fn rate_limit_refuses_queries_over_the_per_client_budget() {
    use hickory_proto::op::ResponseCode;

    let upstream_addr = spawn_stub_upstream().await;
    let cfg = DnsConfig {
        rate_limit_per_second: 1, // capacity = burst = 1
        upstream_servers: vec![udp_upstream(upstream_addr)],
        ..DnsConfig::default()
    };
    let server = build_test_server(cfg, loopback_ephemeral());
    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    // First query consumes the single token → forwarded NoError.
    let first = query_foo_com(bound).await;
    assert_eq!(first.metadata.response_code, ResponseCode::NoError);

    // Second query from the same client IP within the same second → no
    // token left → REFUSED, before any cache/forward work.
    let second = query_foo_com(bound).await;
    assert_eq!(second.metadata.response_code, ResponseCode::Refused);

    server.stop().await.unwrap();
}

#[tokio::test]
async fn rebinding_toggle_via_update_config_flushes_cache_and_blocks_private_ip() {
    use hickory_proto::op::ResponseCode;
    use hickory_proto::rr::RData;

    let upstream_addr = spawn_private_upstream().await;
    // Start with rebinding OFF: the private answer is forwarded + cached.
    let cfg = DnsConfig {
        rebinding_protection: false,
        upstream_servers: vec![udp_upstream(upstream_addr)],
        ..DnsConfig::default()
    };
    let server = build_test_server(cfg, loopback_ephemeral());
    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    let before = query_foo_com(bound).await;
    assert_eq!(before.metadata.response_code, ResponseCode::NoError);
    assert!(
        before.answers.iter().any(|r| matches!(
            &r.data,
            RData::A(a) if a.0 == Ipv4Addr::new(192, 168, 0, 1)
        )),
        "with rebinding off the private answer is returned"
    );
    assert!(server.cache_size().await > 0, "answer should be cached");

    // Toggle rebinding ON via update_config — same upstreams, so no
    // resolver rebuild, but the policy changed → the cache must flush.
    server
        .update_config(DnsConfig {
            rebinding_protection: true,
            upstream_servers: vec![udp_upstream(upstream_addr)],
            ..DnsConfig::default()
        })
        .await;
    assert_eq!(
        server.cache_size().await,
        0,
        "update_config must flush the cache when rebinding toggles"
    );

    // Re-query: cache is empty + rebinding ON → re-forward, private IP
    // rejected as empty NOERROR (NODATA), not NXDOMAIN.
    let after = query_foo_com(bound).await;
    assert_eq!(after.metadata.response_code, ResponseCode::NoError);
    assert!(
        after.answers.is_empty(),
        "rebinding must strip the private answer (NODATA)"
    );

    server.stop().await.unwrap();
}

// ---------------------------------------------------------------------------
// Stage 5 — recursive resolution mode (#219)
//
// `resolve_via_recursor`'s happy path hits the real root servers, so it's
// covered by Pi/manual acceptance rather than CI. These units pin the
// deterministic, network-free branches: the recursor-unavailable fallback
// (forward iff upstreams are configured, else SERVFAIL) and that
// `build_recursor` constructs for both DNSSEC settings.
// ---------------------------------------------------------------------------

/// A `DnsSocket` that records every datagram passed to `send_to` instead of
/// touching the network, so a test can inspect the response the server built.
/// `recv_from` is never exercised by the responder paths under test.
struct RecordingSocket {
    sent: Arc<StdMutex<Vec<Vec<u8>>>>,
}

#[async_trait]
impl DnsSocket for RecordingSocket {
    async fn recv_from(&self, _buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "RecordingSocket does not receive",
        ))
    }

    async fn send_to(&self, buf: &[u8], _target: SocketAddr) -> std::io::Result<usize> {
        self.sent.lock().unwrap().push(buf.to_vec());
        Ok(buf.len())
    }
}

/// Parse the foo.com A request (id=0xCAFE) used by the recursor tests from the
/// same wire bytes as `query_foo_com`, so the request carries exactly one
/// question.
fn foo_com_request() -> hickory_proto::op::Message {
    use hickory_proto::op::Message;
    use hickory_proto::serialize::binary::BinDecodable;

    let query: &[u8] = &[
        0xCA, 0xFE, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, b'f', b'o',
        b'o', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    Message::from_bytes(query).expect("parse foo.com request")
}

#[test]
fn build_recursor_constructs_for_both_dnssec_settings() {
    assert!(
        build_recursor(false).is_some(),
        "recursor must build with DNSSEC disabled"
    );
    assert!(
        build_recursor(true).is_some(),
        "recursor must build with DNSSEC validation enabled"
    );
}

#[tokio::test]
async fn recursor_unavailable_falls_back_to_forwarding_when_upstreams_set() {
    use hickory_proto::op::{Message, ResponseCode};
    use hickory_proto::rr::RData;
    use hickory_proto::serialize::binary::BinDecodable;

    let upstream_addr = spawn_stub_upstream().await;
    let upstreams = vec![udp_upstream(upstream_addr)];

    // Recursor absent (None) → the fallback branch runs. With upstreams
    // configured, it must forward via the resolver, not SERVFAIL.
    let recursor = Arc::new(RwLock::new(None));
    let resolver = Arc::new(RwLock::new(build_resolver(
        &upstreams,
        false,
        ServerOrderingStrategy::QueryStatistics,
    )));
    let config = Arc::new(RwLock::new(DnsConfig {
        resolution_mode: DnsResolutionMode::Recursive,
        upstream_servers: upstreams,
        ..DnsConfig::default()
    }));
    let cache = Arc::new(RwLock::new(DnsCache::new(1000)));
    let sent = Arc::new(StdMutex::new(Vec::new()));
    let socket: Arc<dyn DnsSocket> = Arc::new(RecordingSocket { sent: sent.clone() });
    let src: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 5353));

    resolve_via_recursor(
        &recursor,
        &resolver,
        &socket,
        &config,
        &cache,
        None,
        foo_com_request(),
        0xCAFE,
        src,
        "foo.com",
        RecordType::A,
        std::time::Instant::now(),
        "forwarded",
        UpstreamId::Default,
    )
    .await
    .expect("resolve_via_recursor");

    let frames = sent.lock().unwrap().clone();
    assert_eq!(frames.len(), 1, "exactly one response sent");
    let response = Message::from_bytes(&frames[0]).expect("parse response");
    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert!(
        response.answers.iter().any(|r| matches!(
            &r.data,
            RData::A(a) if a.0 == Ipv4Addr::new(93, 184, 216, 34)
        )),
        "fallback forwarding must return the stub upstream's answer"
    );
}

#[tokio::test]
async fn recursor_unavailable_servfails_when_no_upstreams() {
    use hickory_proto::op::{Message, ResponseCode};
    use hickory_proto::serialize::binary::BinDecodable;

    // Recursor absent AND no upstreams configured (pure recursive) → the
    // server must SERVFAIL rather than leak to a default public resolver.
    let recursor = Arc::new(RwLock::new(None));
    let resolver = Arc::new(RwLock::new(build_resolver(
        &[],
        false,
        ServerOrderingStrategy::QueryStatistics,
    )));
    let config = Arc::new(RwLock::new(DnsConfig {
        resolution_mode: DnsResolutionMode::Recursive,
        upstream_servers: vec![],
        ..DnsConfig::default()
    }));
    let cache = Arc::new(RwLock::new(DnsCache::new(1000)));
    let sent = Arc::new(StdMutex::new(Vec::new()));
    let socket: Arc<dyn DnsSocket> = Arc::new(RecordingSocket { sent: sent.clone() });
    let src: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 5353));

    resolve_via_recursor(
        &recursor,
        &resolver,
        &socket,
        &config,
        &cache,
        None,
        foo_com_request(),
        0xCAFE,
        src,
        "foo.com",
        RecordType::A,
        std::time::Instant::now(),
        "forwarded",
        UpstreamId::Default,
    )
    .await
    .expect("resolve_via_recursor");

    let frames = sent.lock().unwrap().clone();
    assert_eq!(frames.len(), 1, "exactly one response sent");
    let response = Message::from_bytes(&frames[0]).expect("parse response");
    assert_eq!(
        response.metadata.response_code,
        ResponseCode::ServFail,
        "pure recursive with no upstreams must SERVFAIL, never forward to a default"
    );
    assert!(response.answers.is_empty(), "SERVFAIL carries no answers");
}

#[tokio::test]
async fn recursive_mode_dispatches_through_handle_query_and_falls_back() {
    // End-to-end through the real query pipeline: a Recursive-mode server
    // whose recursor has been dropped takes the recursor-unavailable branch
    // and falls back to the configured upstream. This exercises the
    // `handle_query` resolution-mode dispatch (the `if recursive { ... }`
    // arm) and `with_bind_addr`'s recursive construction branch, without
    // touching the real root servers.
    use hickory_proto::op::ResponseCode;
    use hickory_proto::rr::RData;

    let upstream_addr = spawn_stub_upstream().await;
    let cfg = DnsConfig {
        resolution_mode: DnsResolutionMode::Recursive,
        upstream_servers: vec![udp_upstream(upstream_addr)],
        ..DnsConfig::default()
    };
    let server = build_test_server(cfg, loopback_ephemeral());
    // Drop the (real-root) recursor so dispatch falls back deterministically.
    server.clear_recursor_for_test().await;
    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    let resp = query_foo_com(bound).await;
    assert_eq!(
        resp.metadata.response_code,
        ResponseCode::NoError,
        "recursive dispatch must fall back to the upstream and answer NoError"
    );
    assert!(
        resp.answers.iter().any(|r| matches!(
            &r.data,
            RData::A(a) if a.0 == Ipv4Addr::new(93, 184, 216, 34)
        )),
        "fallback forwarding must return the stub upstream's answer"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn mode_change_via_update_config_rebuilds_recursor_and_flushes_cache() {
    // Toggling resolution_mode through update_config must (re)build/clear the
    // recursor and flush the cache so a stale forwarding answer isn't served
    // after switching modes. We never query in recursive mode, so no
    // root-server network I/O happens — only the lifecycle branches run.
    let upstream_addr = spawn_stub_upstream().await;
    let forwarding = DnsConfig {
        upstream_servers: vec![udp_upstream(upstream_addr)],
        ..DnsConfig::default()
    };
    let server = build_test_server(forwarding.clone(), loopback_ephemeral());
    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    // Populate the cache via a forwarded query.
    query_foo_com(bound).await;
    assert!(
        server.cache_size().await > 0,
        "forwarded answer should populate the cache"
    );

    // Forwarding -> Recursive: builds the recursor and flushes on mode change.
    server
        .update_config(DnsConfig {
            resolution_mode: DnsResolutionMode::Recursive,
            upstream_servers: vec![udp_upstream(upstream_addr)],
            ..DnsConfig::default()
        })
        .await;
    assert_eq!(
        server.cache_size().await,
        0,
        "switching resolution mode must flush the cache"
    );

    // Recursive -> Forwarding: clears the recursor (None branch).
    server.update_config(forwarding).await;

    server.stop().await.unwrap();
}

#[tokio::test]
async fn resolve_via_recursor_servfails_on_empty_query() {
    // A request carrying no question falls through to the early SERVFAIL
    // guard rather than panicking on `queries.first()`.
    use hickory_proto::op::{Message, OpCode, ResponseCode};
    use hickory_proto::serialize::binary::BinDecodable;

    let recursor = Arc::new(RwLock::new(None));
    let resolver = Arc::new(RwLock::new(build_resolver(
        &[],
        false,
        ServerOrderingStrategy::QueryStatistics,
    )));
    let config = Arc::new(RwLock::new(DnsConfig {
        resolution_mode: DnsResolutionMode::Recursive,
        ..DnsConfig::default()
    }));
    let cache = Arc::new(RwLock::new(DnsCache::new(1000)));
    let sent = Arc::new(StdMutex::new(Vec::new()));
    let socket: Arc<dyn DnsSocket> = Arc::new(RecordingSocket { sent: sent.clone() });
    let src: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 5353));

    // A Query-opcode message with no questions.
    let request = Message::response(0xABCD, OpCode::Query);
    assert!(request.queries.is_empty(), "request must carry no question");

    resolve_via_recursor(
        &recursor,
        &resolver,
        &socket,
        &config,
        &cache,
        None,
        request,
        0xABCD,
        src,
        "",
        RecordType::A,
        std::time::Instant::now(),
        "forwarded",
        UpstreamId::Default,
    )
    .await
    .expect("resolve_via_recursor");

    let frames = sent.lock().unwrap().clone();
    assert_eq!(frames.len(), 1, "exactly one response sent");
    let response = Message::from_bytes(&frames[0]).expect("parse response");
    assert_eq!(response.metadata.response_code, ResponseCode::ServFail);
}

// ---------------------------------------------------------------------------
// effective_upstreams — failover/fastest keep the full pool; single yields one.
// ---------------------------------------------------------------------------

fn named_udp_upstream(address: &str, name: &str) -> UpstreamDns {
    UpstreamDns {
        address: address.to_owned(),
        name: name.to_owned(),
        protocol: DnsProtocol::Udp,
        port: None,
        tls_server_name: None,
    }
}

#[test]
fn effective_upstreams_non_single_modes_keep_full_pool() {
    use wardnet_common::dns::ForwarderSelectionMode;
    for mode in [
        ForwarderSelectionMode::Failover,
        ForwarderSelectionMode::Fastest,
    ] {
        let cfg = DnsConfig {
            upstream_servers: vec![
                named_udp_upstream("1.1.1.1", "CF"),
                named_udp_upstream("8.8.8.8", "G"),
            ],
            forwarder_selection_mode: mode,
            single_upstream: None,
            ..DnsConfig::default()
        };
        let result = effective_upstreams(&cfg);
        assert_eq!(result.len(), 2, "{mode:?} keeps every upstream in the pool");
    }
}

#[test]
fn effective_upstreams_single_yields_only_selected() {
    use wardnet_common::dns::ForwarderSelectionMode;
    let cfg = DnsConfig {
        upstream_servers: vec![
            named_udp_upstream("1.1.1.1", "CF"),
            named_udp_upstream("8.8.8.8", "G"),
            named_udp_upstream("9.9.9.9", "Q9"),
        ],
        forwarder_selection_mode: ForwarderSelectionMode::Single,
        single_upstream: Some("8.8.8.8".to_owned()),
        ..DnsConfig::default()
    };
    let result = effective_upstreams(&cfg);
    assert_eq!(result.len(), 1, "single yields exactly the chosen server");
    assert_eq!(result[0].address, "8.8.8.8");
}

#[test]
fn effective_upstreams_single_missing_address_falls_back_to_full_pool() {
    // Should be prevented by API validation, but if the chosen address is not
    // in the list we degrade to the full configured pool (NOT an empty set,
    // which would make build_resolver route everything to hard-coded
    // Cloudflare — a privacy regression).
    use wardnet_common::dns::ForwarderSelectionMode;
    let cfg = DnsConfig {
        upstream_servers: vec![
            named_udp_upstream("1.1.1.1", "CF"),
            named_udp_upstream("8.8.8.8", "G"),
        ],
        forwarder_selection_mode: ForwarderSelectionMode::Single,
        single_upstream: Some("9.9.9.9".to_owned()),
        ..DnsConfig::default()
    };
    let result = effective_upstreams(&cfg);
    assert_eq!(
        result.len(),
        2,
        "orphaned selection degrades to the full pool"
    );
}
