//! Feature-gated bench seam for the DNS resolution pipeline.
//!
//! Compiled only under the `bench-internals` feature — NEVER in a production
//! build, and deliberately NOT under a plain `cfg(test)` (no test uses it), so
//! its bench-only stubs stay out of the coverage build. It exposes just enough
//! of the crate-private query path
//! for `benches/dns_resolution.rs` to stand a [`QueryPipeline`] up and drive it
//! directly: an in-process UDP stub upstream, a settable filter, empty routing
//! and device snapshots, a no-op tunnel repository, and helpers to populate the
//! authoritative view and encode a query packet.
//!
//! Nothing here touches a real socket beyond the loopback stub, an upstream
//! resolver aimed at that stub, or a database — the goal is to isolate the
//! resolve core's own cost. Mirrors the `fuzz-internals` seam in
//! `wardnetd-services`: it ADDS visibility only, and changes no production code
//! path when the feature is off.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use chrono::Utc;
use hickory_proto::op::{Message, OpCode};
use hickory_proto::rr::rdata::A;
use hickory_proto::rr::{Name, RData, Record, RecordType};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use uuid::Uuid;
use wardnet_common::dns::{
    CustomDnsRecord, DnsConfig, DnsProtocol, DnsRecordSource, DnsRecordType, FilterAction,
    UpstreamDns,
};
use wardnet_common::tunnel::{Tunnel, TunnelConfig};
use wardnetd_data::repository::TunnelRepository;
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_services::dns::authoritative::AuthoritativeView;
use wardnetd_services::dns::cache::DnsCache;
use wardnetd_services::dns_filter::service::CheckOutcome;

use crate::dns::pipeline::QueryPipeline;
use crate::dns::rate_limit::RateLimiter;
use crate::dns::upstream_pool::UpstreamPool;

/// A `DnsFilterService` whose hot-path verdict is settable and read lock-free.
///
/// Only [`check`](wardnetd_services::DnsFilterService::check) is exercised by
/// the bench; every admin/repository method is `unimplemented!()` — the bench
/// never calls them. The action lives in an [`ArcSwap`] so flipping it between
/// resolution branches never introduces lock contention on the concurrent hot
/// path.
pub struct BenchFilter {
    action: ArcSwap<FilterAction>,
}

impl BenchFilter {
    /// A filter that starts by passing every query.
    #[must_use]
    pub fn passing() -> Self {
        Self {
            action: ArcSwap::from_pointee(FilterAction::Pass),
        }
    }

    /// Swap the verdict every subsequent `check` returns.
    pub fn set(&self, action: FilterAction) {
        self.action.store(Arc::new(action));
    }
}

#[async_trait]
impl wardnetd_services::DnsFilterService for BenchFilter {
    async fn check(&self, _domain: &str, _qtype: RecordType, _client: IpAddr) -> CheckOutcome {
        CheckOutcome {
            action: **self.action.load(),
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

/// No-op `TunnelRepository`: knows no tunnels, so the tunnel-forward path is
/// never taken. Present only to satisfy `QueryPipeline::new`.
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

/// The single A record `spawn_stub_upstream` answers with (`example.com`'s
/// historical address); public, so DNS-rebinding protection never trips.
const STUB_ANSWER_IP: Ipv4Addr = Ipv4Addr::new(93, 184, 216, 34);

/// Spawn a loopback UDP responder that answers every A question with
/// [`STUB_ANSWER_IP`] at TTL 60 (so the answer is cacheable), and echoes an
/// empty NOERROR for anything else. Returns the bound address; the task
/// self-terminates when the runtime is dropped.
///
/// This is the bench's stand-in for a real upstream — an in-process socket,
/// so the "forward to upstream" branch measures the pipeline plus a loopback
/// round-trip rather than the internet.
pub async fn spawn_stub_upstream() -> SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("stub upstream bind");
    let addr = socket.local_addr().expect("stub upstream local_addr");
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            let Ok((n, peer)) = socket.recv_from(&mut buf).await else {
                break;
            };
            let Ok(request) = Message::from_bytes(&buf[..n]) else {
                continue;
            };
            let mut response = Message::response(request.metadata.id, OpCode::Query);
            response.metadata.recursion_desired = true;
            response.metadata.recursion_available = true;
            response.add_queries(request.queries.clone());
            for q in &request.queries {
                if q.query_type() == RecordType::A {
                    response.add_answer(Record::from_rdata(
                        q.name().clone(),
                        60,
                        RData::A(A(STUB_ANSWER_IP)),
                    ));
                }
            }
            if let Ok(bytes) = response.to_bytes() {
                let _ = socket.send_to(&bytes, peer).await;
            }
        }
    });
    addr
}

/// Wire-encode a one-question query, ready to hand to
/// [`QueryPipeline::handle`](crate::dns::pipeline::QueryPipeline::handle).
#[must_use]
pub fn query_bytes(id: u16, name: &str, rtype: RecordType) -> Vec<u8> {
    use hickory_proto::op::Query;

    let mut msg = Message::query();
    msg.metadata.id = id;
    msg.metadata.recursion_desired = true;
    msg.add_queries(vec![Query::query(
        Name::from_str_relaxed(name).expect("query name"),
        rtype,
    )]);
    msg.to_bytes().expect("encode query")
}

/// A single enabled manual A record for `domain` → `ip`, for seeding the
/// authoritative view.
#[must_use]
pub fn a_record(domain: &str, ip: &str) -> CustomDnsRecord {
    CustomDnsRecord {
        id: Uuid::new_v4(),
        zone_id: None,
        domain: domain.to_ascii_lowercase(),
        record_type: DnsRecordType::A,
        value: ip.into(),
        ttl: 120,
        enabled: true,
        source: DnsRecordSource::Manual,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// A `DnsConfig` whose sole upstream is the stub at `addr` (plaintext UDP),
/// with rate limiting disabled and every other field at its default.
#[must_use]
pub fn config_with_upstream(addr: SocketAddr) -> DnsConfig {
    DnsConfig {
        upstream_servers: vec![UpstreamDns {
            name: "bench-stub".into(),
            address: addr.ip().to_string(),
            protocol: DnsProtocol::Udp,
            port: Some(addr.port()),
            tls_server_name: None,
        }],
        rate_limit_per_second: 0,
        ..DnsConfig::default()
    }
}

/// A `QueryPipeline` wired for benchmarking, plus the handles a bench needs to
/// steer it down a specific branch.
pub struct BenchPipeline {
    pipeline: Arc<QueryPipeline>,
    filter: Arc<BenchFilter>,
}

impl BenchPipeline {
    /// The pipeline under test. Call
    /// [`handle`](crate::dns::pipeline::QueryPipeline::handle) on it.
    #[must_use]
    pub fn pipeline(&self) -> &Arc<QueryPipeline> {
        &self.pipeline
    }

    /// Set the verdict the filter returns for every subsequent query — used
    /// to steer into the block and rewrite branches.
    pub fn set_filter_action(&self, action: FilterAction) {
        self.filter.set(action);
    }

    /// Swap in an authoritative view built from `records`, so a query for one
    /// of those names is answered authoritatively (before cache and filter).
    pub fn set_authoritative_records(&self, records: Vec<CustomDnsRecord>) {
        self.pipeline
            .authoritative_view
            .store(Arc::new(AuthoritativeView::build(&[], records, vec![])));
    }
}

/// Build a benchmark pipeline whose upstream is the stub at `upstream`. The
/// routing and device snapshots start empty, the tunnel repository is a no-op,
/// and the rate limiter is disabled — so `handle` measures the resolve core
/// itself rather than admission control or per-client bookkeeping.
#[must_use]
pub fn build_bench_pipeline(upstream: SocketAddr) -> BenchPipeline {
    let cfg = config_with_upstream(upstream);
    let pool = Arc::new(arc_swap::ArcSwap::from_pointee(UpstreamPool::build(&cfg)));
    let rate_limiter = Arc::new(RateLimiter::new(0));
    let cache = Arc::new(RwLock::new(DnsCache::new(cfg.cache_size as usize)));
    let filter = Arc::new(BenchFilter::passing());

    let pipeline = QueryPipeline::new(
        Arc::new(RwLock::new(cfg)),
        pool,
        Arc::new(RwLock::new(None)),
        rate_limiter,
        cache,
        Arc::clone(&filter) as Arc<dyn wardnetd_services::DnsFilterService>,
        None,
        Arc::new(ArcSwap::from_pointee(HashMap::new())),
        Arc::new(ArcSwap::from_pointee(HashMap::new())),
        Arc::new(ArcSwap::from_pointee(HashMap::new())),
        stub_tunnel_repo(),
    );

    BenchPipeline {
        pipeline: Arc::new(pipeline),
        filter,
    }
}
