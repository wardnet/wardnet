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
use wardnet_common::dns::{DnsConfig, DnsProtocol, DnsResolutionMode, UpstreamDns, UpstreamId};
use wardnet_common::event::WardnetEvent;
use wardnet_common::tunnel::{Tunnel, TunnelConfig, TunnelStatus};
use wardnet_common::wireguard_config::WgPeerConfig;
use wardnetd_data::repository::TunnelRepository;
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_services::dns::UpstreamHealth;
use wardnetd_services::dns::cache::DnsCache;
use wardnetd_services::dns::server::{DnsServer, DnsSocket};
use wardnetd_services::event::{BroadcastEventBus, EventPublisher};

use crate::dns::pipeline::{QueryAttribution, TransportProtocol};
use crate::dns::server::{
    LATENCY_PROBE_INTERVAL, TunnelForwarderInfo, UdpDnsServer, build_recursor, duration_to_ms,
    fold_probe_outcomes, get_or_build_tunnel_forwarder, handle_recursor_outcome, probe_upstreams,
    resolve_via_recursor, spawn_cache_invalidator, spawn_upstream_latency_prober,
};
use crate::dns::upstream_pool::UpstreamPool;
use crate::tests::stubs::StubDnsFilterService;

/// The forwarding ladder for `upstreams`, as the pipeline sees it: one
/// single-server resolver per upstream, all of them serving.
fn test_pool(upstreams: &[UpstreamDns]) -> Arc<ArcSwap<UpstreamPool>> {
    Arc::new(ArcSwap::from_pointee(UpstreamPool::build(&DnsConfig {
        upstream_servers: upstreams.to_vec(),
        ..DnsConfig::default()
    })))
}

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

fn empty_device_routing_snapshot() -> Arc<ArcSwap<HashMap<uuid::Uuid, UpstreamId>>> {
    Arc::new(ArcSwap::from_pointee(HashMap::new()))
}

fn empty_device_snapshot() -> Arc<ArcSwap<HashMap<IpAddr, uuid::Uuid>>> {
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
        None,
        empty_routing_snapshot(),
        empty_device_routing_snapshot(),
        empty_device_snapshot(),
        stub_tunnel_repo(),
        stub_events(),
        Arc::new(UpstreamHealth::new()),
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
        QueryAttribution {
            device_id: None,
            protocol: TransportProtocol::Udp,
        },
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
        QueryAttribution {
            device_id: None,
            protocol: TransportProtocol::Dot,
        },
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
    assert_eq!(row.protocol, "dot");
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
        None,
        empty_routing_snapshot(),
        empty_device_routing_snapshot(),
        empty_device_snapshot(),
        stub_tunnel_repo(),
        stub_events(),
        Arc::new(UpstreamHealth::new()),
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
        None,
        empty_routing_snapshot(),
        empty_device_routing_snapshot(),
        empty_device_snapshot(),
        stub_tunnel_repo(),
        stub_events(),
        Arc::new(UpstreamHealth::new()),
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
    // No upstream served this query, so the column stays empty. It used to
    // name `upstream_servers[0]` unconditionally (#1199), which meant a whole
    // outage was attributed to whichever server happened to be listed first
    // — the log pointed diagnosis at the wrong provider precisely when it
    // mattered. Which servers failed is in the per-upstream warnings instead.
    assert!(
        row.upstream.is_none(),
        "an exhausted ladder blames no upstream, got {:?}",
        row.upstream
    );

    server.stop().await.unwrap();
}

// ---------------------------------------------------------------------------
// The forwarding ladder (#1199): bounded deadlines, real failover, and a log
// row that names the upstream that actually answered.
//
// These drive the full UDP path against real sockets, so they need an
// upstream that accepts datagrams and never replies — a closed port or an
// unrouteable address would fail fast with an ICMP error and never exercise
// the timeout. `spawn_black_hole_upstream` is that server.
//
// The timings come from config rather than the defaults so the tests finish
// in fractions of a second while still asserting the bound that is actually
// configured. The defaults themselves (1.5s / 3.5s) are asserted separately.
// ---------------------------------------------------------------------------

/// An upstream that answers every query with NXDOMAIN.
///
/// `spawn_stub_upstream` answers *any* A query with a record, so it cannot
/// exercise the negative-answer path; this one can.
async fn spawn_nxdomain_upstream() -> SocketAddr {
    use hickory_proto::op::{Message, OpCode, ResponseCode};
    use hickory_proto::serialize::binary::BinDecodable;

    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("nxdomain upstream bind");
    let addr = socket.local_addr().expect("nxdomain upstream local_addr");
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            let Ok((n, src)) = socket.recv_from(&mut buf).await else {
                break;
            };
            let Ok(request) = Message::from_bytes(&buf[..n]) else {
                continue;
            };
            let mut response = Message::response(request.metadata.id, OpCode::Query);
            response.metadata.recursion_desired = true;
            response.metadata.recursion_available = true;
            response.metadata.response_code = ResponseCode::NXDomain;
            response.add_queries(request.queries.clone());
            if let Ok(bytes) = response.to_vec() {
                let _ = socket.send_to(&bytes, src).await;
            }
        }
    });
    addr
}

/// An upstream that swallows every query and never answers.
async fn spawn_black_hole_upstream() -> SocketAddr {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("black hole bind");
    let addr = socket.local_addr().expect("black hole local_addr");
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        while socket.recv_from(&mut buf).await.is_ok() {
            // Deliberately no reply.
        }
    });
    addr
}

/// A config whose ladder gives each upstream `per_upstream_ms` and the whole
/// query `deadline_ms`.
fn ladder_config(upstreams: Vec<UpstreamDns>, per_upstream_ms: u32, deadline_ms: u32) -> DnsConfig {
    DnsConfig {
        upstream_servers: upstreams,
        upstream_timeout_ms: per_upstream_ms,
        forward_deadline_ms: deadline_ms,
        ..DnsConfig::default()
    }
}

fn passing_filter() -> Arc<dyn wardnetd_services::DnsFilterService> {
    Arc::new(ConfigurableFilter {
        action: wardnet_common::dns::FilterAction::Pass,
    })
}

#[tokio::test]
async fn the_ladder_fails_over_and_logs_the_upstream_that_answered() {
    // A dead first upstream must not fail the query, and the log row must
    // name the server that actually served it. Before #1199 this row always
    // said `upstream_servers[0]` — the dead one.
    let (sink, mut rx) = wardnetd_services::dns::log_sink::DnsLogSink::new();
    let dead = spawn_black_hole_upstream().await;
    let alive = spawn_stub_upstream().await;
    let cfg = ladder_config(vec![udp_upstream(dead), udp_upstream(alive)], 150, 2_000);
    let server = build_with_filter(cfg, passing_filter(), Arc::clone(&sink));

    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");
    let response = query_foo_com(bound).await;

    assert_eq!(
        response.metadata.response_code,
        hickory_proto::op::ResponseCode::NoError,
        "the second upstream answers, so the client gets a real answer"
    );

    let row = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("row arrives")
        .unwrap();
    assert_eq!(row.result, "forwarded");
    assert_eq!(
        row.upstream.as_deref(),
        Some(alive.ip().to_string().as_str()),
        "the row names the upstream that answered, not the first in the list"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn every_upstream_dead_servfails_within_the_deadline() {
    // The acceptance criterion behind #1199: with every upstream silent the
    // client gets an answer within a bounded time, rather than the 20-31s the
    // incident recorded — by which point the stub resolver had long given up
    // and was retransmitting into our rate limiter.
    let (sink, mut rx) = wardnetd_services::dns::log_sink::DnsLogSink::new();
    let first = spawn_black_hole_upstream().await;
    let second = spawn_black_hole_upstream().await;
    let third = spawn_black_hole_upstream().await;
    let deadline_ms = 900;
    let cfg = ladder_config(
        vec![
            udp_upstream(first),
            udp_upstream(second),
            udp_upstream(third),
        ],
        200,
        deadline_ms,
    );
    let server = build_with_filter(cfg, passing_filter(), Arc::clone(&sink));

    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    let started = std::time::Instant::now();
    let response = query_foo_com(bound).await;
    let elapsed = started.elapsed();

    assert_eq!(
        response.metadata.response_code,
        hickory_proto::op::ResponseCode::ServFail,
        "no upstream answered, so the client gets SERVFAIL"
    );
    // Generous slack over the configured ceiling: this asserts the deadline
    // exists and is enforced, not that the scheduler is punctual.
    assert!(
        elapsed < Duration::from_millis(u64::from(deadline_ms)) + Duration::from_secs(2),
        "forwarding must be bounded by the deadline, took {elapsed:?}"
    );

    let row = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("row arrives")
        .unwrap();
    assert_eq!(row.result, "upstream_error");
    assert!(
        row.upstream.is_none(),
        "no upstream served the query, so none is named"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn a_negative_answer_is_terminal_and_does_not_fail_over() {
    // NXDOMAIN is a resolution, not a failure. Failing over on it would ask a
    // second provider a question the first already answered — wasting a
    // round-trip, leaking the name to another provider, and risking a
    // contradictory answer.
    let (sink, mut rx) = wardnetd_services::dns::log_sink::DnsLogSink::new();
    let first = spawn_nxdomain_upstream().await;
    let never_reached = spawn_black_hole_upstream().await;
    let cfg = ladder_config(
        vec![udp_upstream(first), udp_upstream(never_reached)],
        200,
        2_000,
    );
    let server = build_with_filter(cfg, passing_filter(), Arc::clone(&sink));

    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    let started = std::time::Instant::now();
    let response = query_a_named(bound, "nope.example.", 0xBEEF).await;
    let elapsed = started.elapsed();

    assert_eq!(
        response.metadata.response_code,
        hickory_proto::op::ResponseCode::NXDomain,
        "the negative answer is relayed to the client, not turned into SERVFAIL"
    );
    // Had it fallen through to the black hole, this would have cost at least
    // the per-upstream timeout before answering.
    assert!(
        elapsed < Duration::from_millis(200),
        "a negative answer must return immediately, took {elapsed:?}"
    );

    let row = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("row arrives")
        .unwrap();
    assert_eq!(
        row.upstream.as_deref(),
        Some(first.ip().to_string().as_str()),
        "the upstream that gave the negative answer is the one recorded"
    );

    server.stop().await.unwrap();
}

#[test]
fn the_default_timings_leave_room_for_two_upstreams_under_a_stub_resolvers_patience() {
    let cfg = DnsConfig::default();
    assert!(
        cfg.upstream_timeout_ms * 2 <= cfg.forward_deadline_ms,
        "the deadline must fit at least two full upstream attempts"
    );
    assert!(
        cfg.forward_deadline_ms < 5_000,
        "the deadline must land inside a glibc stub resolver's ~5s patience"
    );
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
        None,
        snapshot,
        empty_device_routing_snapshot(),
        empty_device_snapshot(),
        tunnel_repo,
        stub_events(),
        Arc::new(UpstreamHealth::new()),
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
        None,
        empty_routing_snapshot(),
        empty_device_routing_snapshot(),
        empty_device_snapshot(),
        stub_tunnel_repo(),
        bus.clone(),
        Arc::new(UpstreamHealth::new()),
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
    use hickory_proto::serialize::binary::BinEncodable;
    let mut answer = Message::response(0, OpCode::Query);
    answer.metadata.recursion_desired = true;
    answer.metadata.recursion_available = true;
    cache.write().await.insert(
        UpstreamId::Default,
        "seed.example.",
        RecordType::A,
        answer.to_bytes().expect("encode seed response"),
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
async fn single_label_query_resolves_via_local_search_domain_hop() {
    use hickory_proto::op::ResponseCode;
    use hickory_proto::rr::RData;
    use wardnet_common::dns::{CustomDnsRecord, DnsRecordSource, DnsRecordType};

    let server = build_test_server(DnsConfig::default(), loopback_ephemeral());
    server.start().await.unwrap();
    let bound = server.local_addr().expect("server bound");

    // Only `wardnet.lan` exists; the client asks for the bare label `wardnet`.
    let zone = lan_zone();
    let record = CustomDnsRecord {
        id: Uuid::new_v4(),
        zone_id: Some(zone.id),
        domain: "wardnet.lan".into(),
        record_type: DnsRecordType::A,
        value: "192.168.1.1".into(),
        ttl: 300,
        enabled: true,
        source: DnsRecordSource::System,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    server
        .update_authoritative_view(AuthoritativeView::build(&[zone], vec![record], vec![]))
        .await;

    // Query bare `wardnet` A (id=0xAB3E) — single label, no dot.
    let query: &[u8] = &[
        0xAB, 0x3E, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, b'w', b'a',
        b'r', b'd', b'n', b'e', b't', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    let resp = send_and_recv(bound, query).await;

    assert_eq!(
        resp.metadata.response_code,
        ResponseCode::NoError,
        "bare `wardnet` must resolve via the local `.lan` search-domain hop"
    );
    assert!(
        resp.metadata.authoritative,
        "the hop answers from the authoritative view, so the AA bit must be set"
    );
    assert!(
        resp.answers.iter().any(|r| matches!(
            &r.data,
            RData::A(a) if a.0 == Ipv4Addr::new(192, 168, 1, 1)
        )),
        "the bare-label answer must carry the `wardnet.lan` A record's address"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn single_label_miss_is_forwarded_not_answered_authoritatively() {
    use hickory_proto::op::ResponseCode;

    // Stub upstream answers NoError+A. A single label with no matching `.lan`
    // record must fall through to it — the hop only adopts a positive hit and
    // never synthesizes NXDOMAIN/NODATA for a bare name.
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

    // Enabled `lan` zone but no `nope.lan` record.
    server
        .update_authoritative_view(AuthoritativeView::build(&[lan_zone()], vec![], vec![]))
        .await;

    // Query bare `nope` A (id=0xAB4E).
    let query: &[u8] = &[
        0xAB, 0x4E, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, b'n', b'o',
        b'p', b'e', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    let resp = send_and_recv(bound, query).await;

    assert_eq!(
        resp.metadata.response_code,
        ResponseCode::NoError,
        "a single-label miss must be forwarded to the upstream, not refused"
    );
    assert!(
        !resp.metadata.authoritative,
        "a forwarded answer must not set the AA bit — proves it was not answered locally"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn single_label_hop_ignores_dhcp_sourced_records() {
    use wardnet_common::dns::{CustomDnsRecord, DnsRecordSource, DnsRecordType};

    // A device-chosen DHCP hostname (`laptop` → `laptop.lan`) must NOT become
    // resolvable at the bare single label `laptop`: the hop only adopts
    // `System`-sourced records, so a device can't claim a bare name (e.g. `wpad`)
    // the client never had a search domain for. The stub upstream answers, so a
    // non-authoritative reply proves the query was forwarded, not answered locally.
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

    // `laptop.lan` exists, but as a DHCP-sourced record (device-supplied name).
    let zone = lan_zone();
    let record = CustomDnsRecord {
        id: Uuid::new_v4(),
        zone_id: Some(zone.id),
        domain: "laptop.lan".into(),
        record_type: DnsRecordType::A,
        value: "192.168.1.77".into(),
        ttl: 300,
        enabled: true,
        source: DnsRecordSource::Dhcp,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    server
        .update_authoritative_view(AuthoritativeView::build(&[zone], vec![record], vec![]))
        .await;

    // Query bare `laptop` A (id=0xAB5E).
    let query: &[u8] = &[
        0xAB, 0x5E, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, b'l', b'a',
        b'p', b't', b'o', b'p', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    let resp = send_and_recv(bound, query).await;

    assert!(
        !resp.metadata.authoritative,
        "a DHCP-sourced `.lan` record must not be adopted for the bare label — the \
         query is forwarded (no AA bit), while `laptop.lan` itself stays resolvable"
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
        "a forwarded query must NOT be answered authoritatively - the forwarding rule overrides zone authority"
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

/// Send an A query for an arbitrary `name` (from a fresh client socket) and
/// return the parsed reply. The companion to [`query_foo_com`] for tests that
/// need a second, distinct question.
async fn query_a_named(target: SocketAddr, name: &str, id: u16) -> hickory_proto::op::Message {
    use hickory_proto::op::{Message, Query};
    use hickory_proto::rr::{Name, RecordType};
    use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};

    let client = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("client bind");
    let mut msg = Message::query();
    msg.metadata.id = id;
    msg.metadata.recursion_desired = true;
    msg.add_queries(vec![Query::query(
        Name::from_str_relaxed(name).expect("query name"),
        RecordType::A,
    )]);
    let bytes = msg.to_bytes().expect("encode query");
    client.send_to(&bytes, target).await.expect("send");
    let mut buf = vec![0u8; 4096];
    let (n, _) = tokio::time::timeout(Duration::from_secs(5), client.recv_from(&mut buf))
        .await
        .expect("client recv timeout")
        .expect("client recv");
    Message::from_bytes(&buf[..n]).expect("parse response")
}

#[tokio::test]
async fn rate_limit_refuses_forwarded_queries_but_not_local_answers() {
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

    // Second query, same domain, same second: the budget is spent, but this is
    // now a cache hit — a local answer — so it is served, NOT refused. The
    // limiter only guards the upstream-bound path.
    let cached = query_foo_com(bound).await;
    assert_eq!(
        cached.metadata.response_code,
        ResponseCode::NoError,
        "a cache hit must not be rate-limited"
    );

    // A different, un-cached domain within the same second would have to be
    // forwarded → no token left → REFUSED.
    let forwarded = query_a_named(bound, "bar.com.", 0xBEEF).await;
    assert_eq!(
        forwarded.metadata.response_code,
        ResponseCode::Refused,
        "a query that must go upstream is rate-limited"
    );

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

/// Build the foo.com request (id=0xCAFE, exactly one question) for the given
/// record type, from the same wire template as `query_foo_com`.
fn foo_com_request_of(rtype: RecordType) -> hickory_proto::op::Message {
    use hickory_proto::op::Message;
    use hickory_proto::serialize::binary::BinDecodable;

    let mut query = [
        0xCA, 0xFE, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, b'f', b'o',
        b'o', 0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01, 0x00, 0x01,
    ];
    query[21..23].copy_from_slice(&u16::from(rtype).to_be_bytes());
    Message::from_bytes(&query).expect("parse foo.com request")
}

/// The foo.com A request used by the recursor tests.
fn foo_com_request() -> hickory_proto::op::Message {
    foo_com_request_of(RecordType::A)
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
    let pool = test_pool(&upstreams);
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
        &pool,
        &socket,
        &config,
        &cache,
        None,
        foo_com_request(),
        0xCAFE,
        src,
        QueryAttribution {
            device_id: None,
            protocol: TransportProtocol::Udp,
        },
        "foo.com",
        RecordType::A,
        std::time::Instant::now(),
        "forwarded",
        UpstreamId::Default,
        None,
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
    let pool = test_pool(&[]);
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
        &pool,
        &socket,
        &config,
        &cache,
        None,
        foo_com_request(),
        0xCAFE,
        src,
        QueryAttribution {
            device_id: None,
            protocol: TransportProtocol::Udp,
        },
        "foo.com",
        RecordType::A,
        std::time::Instant::now(),
        "forwarded",
        UpstreamId::Default,
        None,
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

/// Build a `RecursorError::Negative` like the recursor returns for negative
/// answers (NODATA / NXDOMAIN), carrying the zone's SOA. The SOA record TTL
/// is fixed at 300 while MINIMUM is caller-chosen, so tests can assert the
/// RFC 2308 negative TTL — min(TTL, MINIMUM) — is stamped on the relayed SOA.
fn negative_recursor_error(
    nx_domain: bool,
    soa_minimum: u32,
) -> hickory_resolver::recursor::RecursorError {
    use hickory_proto::op::Query;
    use hickory_proto::rr::rdata::SOA;
    use hickory_proto::rr::{Name, Record};
    use hickory_resolver::recursor::{AuthorityData, RecursorError};

    let name = Name::from_str_relaxed("foo.com.").expect("query name");
    let query = Box::new(Query::query(name, RecordType::A));
    let soa = Record::from_rdata(
        Name::from_str_relaxed("com.").expect("soa name"),
        300,
        SOA::new(
            Name::from_str_relaxed("ns.com.").expect("mname"),
            Name::from_str_relaxed("hostmaster.com.").expect("rname"),
            1,
            3600,
            600,
            86400,
            soa_minimum,
        ),
    );
    RecursorError::Negative(AuthorityData::new(
        query,
        Some(Box::new(soa)),
        true,
        nx_domain,
        None,
    ))
}

/// Shared driver for the negative-answer tests: run `handle_recursor_outcome`
/// with the given recursor outcome and request against a stub upstream that
/// WOULD answer the foo.com A query, so any wrongful fallback shows up as
/// answers. Returns the parsed response plus the cache so tests can assert
/// negative caching. An outcome of `None` exercises the recursor-unavailable
/// fallback-to-forwarder branch.
async fn run_recursor_outcome(
    outcome: Option<Result<hickory_proto::op::Message, hickory_resolver::recursor::RecursorError>>,
    request: hickory_proto::op::Message,
    rtype: RecordType,
    config_tweak: fn(&mut DnsConfig),
) -> (hickory_proto::op::Message, Arc<RwLock<DnsCache>>) {
    use hickory_proto::op::Message;
    use hickory_proto::serialize::binary::BinDecodable;

    let upstream_addr = spawn_stub_upstream().await;
    let upstreams = vec![udp_upstream(upstream_addr)];
    let pool = test_pool(&upstreams);
    let mut cfg = DnsConfig {
        resolution_mode: DnsResolutionMode::Recursive,
        upstream_servers: upstreams,
        ..DnsConfig::default()
    };
    config_tweak(&mut cfg);
    let config = Arc::new(RwLock::new(cfg));
    let cache = Arc::new(RwLock::new(DnsCache::new(1000)));
    let sent = Arc::new(StdMutex::new(Vec::new()));
    let socket: Arc<dyn DnsSocket> = Arc::new(RecordingSocket { sent: sent.clone() });
    let src: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 5353));

    handle_recursor_outcome(
        outcome,
        &pool,
        &socket,
        &config,
        &cache,
        None,
        request,
        0xCAFE,
        src,
        QueryAttribution {
            device_id: None,
            protocol: TransportProtocol::Udp,
        },
        "foo.com",
        rtype,
        std::time::Instant::now(),
        "forwarded",
        UpstreamId::Default,
        None,
    )
    .await
    .expect("handle_recursor_outcome");

    let frames = sent.lock().unwrap().clone();
    assert_eq!(frames.len(), 1, "exactly one response sent");
    let response = Message::from_bytes(&frames[0]).expect("parse response");
    (response, cache)
}

/// Negative recursor outcome (SOA MINIMUM 60) against the foo.com A query.
async fn run_negative_outcome(
    nx_domain: bool,
) -> (hickory_proto::op::Message, Arc<RwLock<DnsCache>>) {
    run_recursor_outcome(
        Some(Err(negative_recursor_error(nx_domain, 60))),
        foo_com_request(),
        RecordType::A,
        |_| {},
    )
    .await
}

#[tokio::test]
async fn recursor_nodata_relays_noerror_not_servfail() {
    use hickory_proto::op::ResponseCode;

    // A NODATA outcome from the recursor is a valid answer ("name exists,
    // no records of this type") — e.g. AAAA for an IPv4-only domain or an
    // HTTPS record most domains lack. The client must see NOERROR with zero
    // answers, not SERVFAIL, and the server must NOT fall back to the
    // forwarder (the stub upstream would answer the A query with a record).
    let (response, cache) = run_negative_outcome(false).await;
    assert_eq!(
        response.metadata.response_code,
        ResponseCode::NoError,
        "NODATA must relay NOERROR, not SERVFAIL or a fallback answer"
    );
    assert!(response.answers.is_empty(), "NODATA carries no answers");
    assert!(
        !response.authorities.is_empty(),
        "negative response must carry the SOA for negative caching"
    );
    assert_eq!(
        response.authorities[0].ttl, 60,
        "relayed SOA must carry the RFC 2308 negative TTL: min(SOA TTL 300, MINIMUM 60)"
    );
    assert!(
        cache
            .write()
            .await
            .get(UpstreamId::Default, "foo.com", RecordType::A, 0)
            .is_some(),
        "negative response must be cached so repeats don't re-resolve"
    );
}

#[tokio::test]
async fn recursor_nxdomain_relays_nxdomain_not_servfail() {
    use hickory_proto::op::ResponseCode;

    let (response, _cache) = run_negative_outcome(true).await;
    assert_eq!(
        response.metadata.response_code,
        ResponseCode::NXDomain,
        "NXDOMAIN must relay NXDOMAIN, not SERVFAIL or a fallback answer"
    );
    assert!(response.answers.is_empty(), "NXDOMAIN carries no answers");
    assert!(
        !response.authorities.is_empty(),
        "NXDOMAIN must carry the SOA for negative caching too"
    );
}

#[tokio::test]
async fn recursor_negative_with_zero_minimum_is_not_cached_or_raised() {
    use hickory_proto::op::ResponseCode;

    // SOA MINIMUM = 0 means the zone forbids negative caching (e.g. ACME
    // DNS-01 zones that publish records seconds after the first query).
    // Even with an admin cache floor configured, the negative must NOT be
    // raised to the floor, cached, or relayed with a non-zero SOA TTL.
    let (response, cache) = run_recursor_outcome(
        Some(Err(negative_recursor_error(false, 0))),
        foo_com_request(),
        RecordType::A,
        |cfg| cfg.cache_ttl_min_secs = 300,
    )
    .await;
    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(
        response.authorities[0].ttl, 0,
        "a zone-forbidden negative TTL must not be raised to the cache floor"
    );
    assert!(
        cache
            .write()
            .await
            .get(UpstreamId::Default, "foo.com", RecordType::A, 0)
            .is_none(),
        "a zone-forbidden negative must not be cached"
    );
}

#[tokio::test]
async fn fallback_forwarder_nodata_relays_noerror_not_servfail() {
    use hickory_proto::op::ResponseCode;

    // Recursor unavailable (outcome None) → fallback forwards to the stub
    // upstream, which returns NOERROR with zero answers for AAAA queries.
    // That negative answer must reach the client as NOERROR/NODATA, not
    // SERVFAIL.
    let (response, _cache) = run_recursor_outcome(
        None,
        foo_com_request_of(RecordType::AAAA),
        RecordType::AAAA,
        |_| {},
    )
    .await;
    assert_eq!(
        response.metadata.response_code,
        ResponseCode::NoError,
        "forwarder NODATA must relay NOERROR, not SERVFAIL"
    );
    assert!(response.answers.is_empty(), "NODATA carries no answers");
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
    let pool = test_pool(&[]);
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
        &pool,
        &socket,
        &config,
        &cache,
        None,
        request,
        0xABCD,
        src,
        QueryAttribution {
            device_id: None,
            protocol: TransportProtocol::Udp,
        },
        "",
        RecordType::A,
        std::time::Instant::now(),
        "forwarded",
        UpstreamId::Default,
        None,
    )
    .await
    .expect("resolve_via_recursor");

    let frames = sent.lock().unwrap().clone();
    assert_eq!(frames.len(), 1, "exactly one response sent");
    let response = Message::from_bytes(&frames[0]).expect("parse response");
    assert_eq!(response.metadata.response_code, ResponseCode::ServFail);
}

// Shared upstream builder for the probe/latency tests below.
fn named_udp_upstream(address: &str, name: &str) -> UpstreamDns {
    UpstreamDns {
        address: address.to_owned(),
        name: name.to_owned(),
        protocol: DnsProtocol::Udp,
        port: None,
        tls_server_name: None,
    }
}

// ---------------------------------------------------------------------------
// fold_probe_outcomes — EWMA + failure-streak/hysteresis + pruning.
// ---------------------------------------------------------------------------

fn find_latency<'a>(
    results: &'a [wardnet_common::dns::UpstreamLatency],
    addr: &str,
) -> &'a wardnet_common::dns::UpstreamLatency {
    results
        .iter()
        .find(|l| l.address == addr)
        .expect("address present in results")
}

#[test]
fn fold_probe_first_success_seeds_ewma_and_is_reachable() {
    let mut ewma = HashMap::new();
    let mut streak = HashMap::new();
    let out = vec![("1.1.1.1".to_owned(), Some(20.0))];
    let results = fold_probe_outcomes(&out, &mut ewma, &mut streak);
    let l = find_latency(&results, "1.1.1.1");
    assert_eq!(l.avg_latency_ms, Some(20.0), "first sample seeds the EWMA");
    assert!(l.reachable);
}

#[test]
fn fold_probe_blends_ewma_across_samples() {
    let mut ewma = HashMap::new();
    let mut streak = HashMap::new();
    // Seed at 20ms, then a 40ms sample: EWMA = 0.3*40 + 0.7*20 = 26.
    fold_probe_outcomes(
        &[("8.8.8.8".to_owned(), Some(20.0))],
        &mut ewma,
        &mut streak,
    );
    let results = fold_probe_outcomes(
        &[("8.8.8.8".to_owned(), Some(40.0))],
        &mut ewma,
        &mut streak,
    );
    let avg = find_latency(&results, "8.8.8.8").avg_latency_ms.unwrap();
    assert!((avg - 26.0).abs() < 1e-9, "EWMA blended: got {avg}");
}

#[test]
fn fold_probe_debounces_unreachable_over_two_misses() {
    let mut ewma = HashMap::new();
    let mut streak = HashMap::new();
    // Establish a latency first.
    fold_probe_outcomes(
        &[("9.9.9.9".to_owned(), Some(15.0))],
        &mut ewma,
        &mut streak,
    );

    // One miss: still reachable (debounce), last-known latency preserved.
    let r1 = fold_probe_outcomes(&[("9.9.9.9".to_owned(), None)], &mut ewma, &mut streak);
    let l1 = find_latency(&r1, "9.9.9.9");
    assert!(l1.reachable, "a single miss must not flip to unreachable");
    assert_eq!(l1.avg_latency_ms, Some(15.0), "keeps last-known latency");

    // Second consecutive miss: now unreachable.
    let r2 = fold_probe_outcomes(&[("9.9.9.9".to_owned(), None)], &mut ewma, &mut streak);
    assert!(
        !find_latency(&r2, "9.9.9.9").reachable,
        "two consecutive misses report unreachable"
    );

    // A success clears the streak → reachable again.
    let r3 = fold_probe_outcomes(
        &[("9.9.9.9".to_owned(), Some(18.0))],
        &mut ewma,
        &mut streak,
    );
    assert!(
        find_latency(&r3, "9.9.9.9").reachable,
        "success resets streak"
    );
}

#[test]
fn fold_probe_prunes_state_for_removed_upstreams() {
    let mut ewma = HashMap::new();
    let mut streak = HashMap::new();
    fold_probe_outcomes(
        &[
            ("1.1.1.1".to_owned(), Some(10.0)),
            ("8.8.8.8".to_owned(), None),
        ],
        &mut ewma,
        &mut streak,
    );
    assert!(ewma.contains_key("1.1.1.1"));
    assert!(streak.contains_key("8.8.8.8"));

    // Next round only has 1.1.1.1 → 8.8.8.8's state is pruned.
    let results = fold_probe_outcomes(
        &[("1.1.1.1".to_owned(), Some(12.0))],
        &mut ewma,
        &mut streak,
    );
    assert_eq!(results.len(), 1);
    assert!(
        !ewma.contains_key("8.8.8.8"),
        "removed upstream pruned from ewma"
    );
    assert!(
        !streak.contains_key("8.8.8.8"),
        "removed upstream pruned from fail_streak"
    );
}

#[test]
fn fold_probe_missing_upstream_with_no_prior_sample_has_no_latency() {
    let mut ewma = HashMap::new();
    let mut streak = HashMap::new();
    let results = fold_probe_outcomes(&[("1.0.0.1".to_owned(), None)], &mut ewma, &mut streak);
    let l = find_latency(&results, "1.0.0.1");
    assert_eq!(l.avg_latency_ms, None, "no sample yet → no latency");
    assert!(
        l.reachable,
        "one miss alone is still within the debounce window"
    );
}

// ---------------------------------------------------------------------------
// probe_upstreams + spawn_upstream_latency_prober — the network/async half.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn probe_upstreams_non_ip_is_miss_and_builds_no_resolver() {
    // A non-IP address can't be probed (build_resolver needs an IP literal),
    // so it's reported as a miss and no resolver is cached for it.
    let mut resolvers = std::collections::HashMap::new();
    let ups = vec![UpstreamDns {
        address: "dns.example.com".to_owned(),
        name: "hostname".to_owned(),
        protocol: DnsProtocol::Udp,
        port: None,
        tls_server_name: None,
    }];
    let out = probe_upstreams(&ups, false, &mut resolvers).await;
    assert_eq!(out, vec![("dns.example.com".to_owned(), None)]);
    assert!(
        resolvers.is_empty(),
        "no resolver built for a non-IP upstream"
    );
}

#[tokio::test]
async fn probe_upstreams_unreachable_ip_is_miss_and_caches_resolver() {
    // 192.0.2.1 is TEST-NET-1 (RFC 5737): reserved and non-routable, so no
    // DNS server can ever answer — the probe deterministically misses (unlike
    // loopback:53, which CI runners answer via systemd-resolved). A resolver
    // is still built and cached for reuse.
    let mut resolvers = std::collections::HashMap::new();
    let ups = vec![named_udp_upstream("192.0.2.1", "test-net")];
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        probe_upstreams(&ups, false, &mut resolvers),
    )
    .await
    .expect("probe_upstreams returns within its own timeout");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, "192.0.2.1");
    assert_eq!(out[0].1, None, "non-routable upstream -> miss");
    assert_eq!(resolvers.len(), 1, "resolver cached for the IP upstream");
}

#[tokio::test(start_paused = true)]
async fn prober_inactive_clears_snapshot_and_sends_no_probe() {
    // DNS disabled -> the forwarding path isn't serving, so the prober must
    // publish an empty snapshot and emit no probe traffic.
    let config = Arc::new(RwLock::new(DnsConfig {
        enabled: false,
        resolution_mode: DnsResolutionMode::Forwarding,
        upstream_servers: vec![named_udp_upstream("127.0.0.1", "loopback")],
        ..DnsConfig::default()
    }));
    let health = Arc::new(UpstreamHealth::new());
    health.publish(vec![wardnet_common::dns::UpstreamLatency {
        address: "stale".to_owned(),
        avg_latency_ms: Some(9.0),
        reachable: true,
    }]);
    let pool = Arc::new(ArcSwap::from_pointee(UpstreamPool::build(
        &config.read().await.clone(),
    )));
    let cancel = tokio_util::sync::CancellationToken::new();
    spawn_upstream_latency_prober(
        Arc::clone(&config),
        Arc::clone(&health),
        Arc::clone(&pool),
        cancel.clone(),
    );

    // Let the spawned task register its interval timer before advancing, then
    // step time forward until the (deferred) first tick fires and the gating
    // branch clears the snapshot. No network is touched on this path.
    tokio::task::yield_now().await;
    let mut cleared = false;
    for _ in 0..60 {
        tokio::time::advance(LATENCY_PROBE_INTERVAL).await;
        tokio::task::yield_now().await;
        if health.snapshot().is_empty() {
            cleared = true;
            break;
        }
    }
    assert!(cleared, "an inactive prober clears the snapshot");
    cancel.cancel();
}

#[tokio::test(start_paused = true)]
async fn prober_active_publishes_snapshot_for_each_upstream() {
    // Active forwarding path: the prober probes each upstream and publishes a
    // per-upstream snapshot. The loopback upstream has no server, so it's an
    // (eventually) reachable=true single-miss entry with no latency yet.
    let config = Arc::new(RwLock::new(DnsConfig {
        enabled: true,
        resolution_mode: DnsResolutionMode::Forwarding,
        upstream_servers: vec![named_udp_upstream("127.0.0.1", "loopback")],
        ..DnsConfig::default()
    }));
    let health = Arc::new(UpstreamHealth::new());
    let pool = Arc::new(ArcSwap::from_pointee(UpstreamPool::build(
        &config.read().await.clone(),
    )));
    let cancel = tokio_util::sync::CancellationToken::new();
    spawn_upstream_latency_prober(
        Arc::clone(&config),
        Arc::clone(&health),
        Arc::clone(&pool),
        cancel.clone(),
    );

    tokio::time::advance(LATENCY_PROBE_INTERVAL).await;
    // Let the probe round run; advance past the per-probe timeout so any
    // timer-based waits inside the lookup resolve to a miss.
    let mut populated = false;
    for _ in 0..40 {
        tokio::task::yield_now().await;
        if !health.snapshot().is_empty() {
            populated = true;
            break;
        }
        tokio::time::advance(std::time::Duration::from_secs(3)).await;
    }
    assert!(
        populated,
        "active prober publishes a snapshot after the first tick"
    );
    let snap = health.snapshot();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].address, "127.0.0.1");
    cancel.cancel();
}

// -- Domain-routing post-resolution hook (#241) ----------------------------

/// A `RoutingProfileService` that records every `note_resolution` call so the
/// pipeline's post-resolution hook can be asserted. Only `note_resolution` is
/// exercised.
#[derive(Default)]
struct RecordingRoutingProfile {
    calls: StdMutex<Vec<(Uuid, String, usize, u32)>>,
}

#[async_trait]
impl wardnetd_services::RoutingProfileService for RecordingRoutingProfile {
    fn note_resolution(
        &self,
        device_id: Uuid,
        _device_ip: IpAddr,
        name: &str,
        answer_ips: &[IpAddr],
        ttl_secs: u32,
    ) {
        self.calls
            .lock()
            .unwrap()
            .push((device_id, name.to_owned(), answer_ips.len(), ttl_secs));
    }

    async fn list_profiles(
        &self,
    ) -> Result<
        Vec<wardnet_common::routing_profile::RoutingProfile>,
        wardnetd_services::error::AppError,
    > {
        unimplemented!("not exercised by the hook test")
    }
    async fn get_profile(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::routing_profile::RoutingProfile, wardnetd_services::error::AppError>
    {
        unimplemented!("not exercised by the hook test")
    }
    async fn create_profile(
        &self,
        _name: &str,
    ) -> Result<wardnet_common::routing_profile::RoutingProfile, wardnetd_services::error::AppError>
    {
        unimplemented!("not exercised by the hook test")
    }
    async fn rename_profile(
        &self,
        _id: Uuid,
        _name: &str,
    ) -> Result<wardnet_common::routing_profile::RoutingProfile, wardnetd_services::error::AppError>
    {
        unimplemented!("not exercised by the hook test")
    }
    async fn delete_profile(&self, _id: Uuid) -> Result<(), wardnetd_services::error::AppError> {
        unimplemented!("not exercised by the hook test")
    }
    async fn list_rules(
        &self,
        _profile_id: Uuid,
    ) -> Result<
        Vec<wardnet_common::routing_profile::DomainRoutingRule>,
        wardnetd_services::error::AppError,
    > {
        unimplemented!("not exercised by the hook test")
    }
    async fn create_rule(
        &self,
        _profile_id: Uuid,
        _pattern: &str,
        _target: wardnet_common::routing_profile::DomainRoutingTarget,
        _enabled: bool,
    ) -> Result<
        wardnet_common::routing_profile::DomainRoutingRule,
        wardnetd_services::error::AppError,
    > {
        unimplemented!("not exercised by the hook test")
    }
    async fn update_rule(
        &self,
        _id: Uuid,
        _pattern: Option<String>,
        _target: Option<wardnet_common::routing_profile::DomainRoutingTarget>,
        _enabled: Option<bool>,
    ) -> Result<
        wardnet_common::routing_profile::DomainRoutingRule,
        wardnetd_services::error::AppError,
    > {
        unimplemented!("not exercised by the hook test")
    }
    async fn delete_rule(&self, _id: Uuid) -> Result<(), wardnetd_services::error::AppError> {
        unimplemented!("not exercised by the hook test")
    }
    async fn get_device_profiles(
        &self,
        _device_id: Uuid,
    ) -> Result<Vec<Uuid>, wardnetd_services::error::AppError> {
        unimplemented!("not exercised by the hook test")
    }
    async fn set_device_profiles(
        &self,
        _device_id: Uuid,
        _profile_ids: &[Uuid],
    ) -> Result<(), wardnetd_services::error::AppError> {
        unimplemented!("not exercised by the hook test")
    }
    async fn list_profile_devices(
        &self,
        _profile_id: Uuid,
    ) -> Result<Vec<Uuid>, wardnetd_services::error::AppError> {
        unimplemented!("not exercised by the hook test")
    }
    async fn refresh_view(&self) -> Result<(), wardnetd_services::error::AppError> {
        unimplemented!("not exercised by the hook test")
    }
}

#[tokio::test]
async fn resolved_answer_notifies_routing_profile_for_attributed_device() {
    use hickory_proto::op::{Message, OpCode};
    use hickory_proto::rr::rdata::A;
    use hickory_proto::rr::{Name, RData, Record};

    // A recursor answer carrying a single public A record.
    let mut answer = Message::response(0xCAFE, OpCode::Query);
    let name = Name::from_ascii("foo.com.").unwrap();
    answer.add_answer(Record::from_rdata(
        name,
        300,
        RData::A(A(Ipv4Addr::new(93, 184, 216, 34))),
    ));

    let upstream_addr = spawn_stub_upstream().await;
    let upstreams = vec![udp_upstream(upstream_addr)];
    let pool = test_pool(&upstreams);
    let config = Arc::new(RwLock::new(DnsConfig {
        resolution_mode: DnsResolutionMode::Recursive,
        upstream_servers: upstreams,
        ..DnsConfig::default()
    }));
    let cache = Arc::new(RwLock::new(DnsCache::new(1000)));
    let sent = Arc::new(StdMutex::new(Vec::new()));
    let socket: Arc<dyn DnsSocket> = Arc::new(RecordingSocket { sent });
    let src: SocketAddr = SocketAddr::from(([10, 0, 0, 5], 5353));
    let device_id = Uuid::new_v4();
    let recorder = Arc::new(RecordingRoutingProfile::default());
    let recorder_dyn: Arc<dyn wardnetd_services::RoutingProfileService> = recorder.clone();

    handle_recursor_outcome(
        Some(Ok(answer)),
        &pool,
        &socket,
        &config,
        &cache,
        None,
        foo_com_request(),
        0xCAFE,
        src,
        QueryAttribution {
            device_id: Some(device_id),
            protocol: TransportProtocol::Udp,
        },
        "foo.com",
        RecordType::A,
        std::time::Instant::now(),
        "forwarded",
        UpstreamId::Default,
        Some(&recorder_dyn),
    )
    .await
    .expect("handle_recursor_outcome");

    // The post-resolution hook queued this device's resolved A record for
    // routing-profile enforcement.
    let calls = recorder.calls.lock().unwrap().clone();
    assert_eq!(calls, vec![(device_id, "foo.com".to_owned(), 1, 300)]);
}
