//! Unit tests for [`QueryPipeline`] driven directly — no UDP listener in
//! the loop, responses captured through [`ReplyCapture`] exactly the way
//! a stream transport will consume the pipeline.
//!
//! The server-level tests in `tests/server.rs` cover the same hot path
//! end-to-end over a real socket; these pin the pipeline's own API
//! contract (the zero-send exits, `ClientIdentity` attribution, the
//! redacted `Debug`) and the branches a stock `UdpDnsServer` setup can't
//! steer into deterministically: conditional forwarding, the cache-hit
//! reply, rebinding protection, negative relaying, and the non-A/AAAA
//! authoritative record types.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use chrono::Utc;
use hickory_proto::op::{Message, ResponseCode};
use hickory_proto::rr::RecordType;
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use tokio::sync::RwLock;
use uuid::Uuid;
use wardnet_common::dns::{
    ConditionalForwardingRule, CustomDnsRecord, DnsConfig, DnsProtocol, DnsRecordSource,
    DnsRecordType, UpstreamDns, UpstreamId,
};
use wardnet_common::tunnel::{Tunnel, TunnelConfig};
use wardnetd_data::repository::TunnelRepository;
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_services::dns::authoritative::AuthoritativeView;
use wardnetd_services::dns::cache::DnsCache;
use wardnetd_services::dns::log_sink::DnsLogSink;
use wardnetd_services::dns::server::DnsSocket;

use crate::dns::pipeline::{ClientIdentity, QueryPipeline, TransportProtocol};
use crate::dns::rate_limit::RateLimiter;
use crate::dns::reply_capture::ReplyCapture;
use crate::dns::server::build_forwarding_resolver;
use crate::tests::stubs::StubDnsFilterService;

fn src() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 5353))
}

pub(super) fn stub_tunnel_repo() -> Arc<dyn TunnelRepository> {
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

/// Build a pipeline around `cfg` with the stock stubs. The authoritative
/// view and routing snapshot start empty; tests that need them populated
/// store into the returned pipeline's `ArcSwap` fields directly (the
/// same way the server's `update_authoritative_view` does).
/// Construct a pipeline over stub state, before wrapping in `Arc` — the shared
/// core of [`build_pipeline`] and [`build_pipeline_with_timeout`].
fn make_pipeline(cfg: DnsConfig, sink: Option<Arc<DnsLogSink>>) -> QueryPipeline {
    let resolver = Arc::new(RwLock::new(build_forwarding_resolver(&cfg)));
    let rate_limiter = Arc::new(RateLimiter::new(cfg.rate_limit_per_second));
    let cache = Arc::new(RwLock::new(DnsCache::new(cfg.cache_size as usize)));
    let mut pipeline = QueryPipeline::new(
        Arc::new(RwLock::new(cfg)),
        resolver,
        Arc::new(RwLock::new(None)),
        rate_limiter,
        cache,
        Arc::new(StubDnsFilterService),
        None,
        Arc::new(ArcSwap::from_pointee(HashMap::new())),
        Arc::new(ArcSwap::from_pointee(HashMap::new())),
        Arc::new(ArcSwap::from_pointee(HashMap::new())),
        stub_tunnel_repo(),
    );
    pipeline.log_sink = sink;
    pipeline
}

pub(super) fn build_pipeline(cfg: DnsConfig, sink: Option<Arc<DnsLogSink>>) -> Arc<QueryPipeline> {
    Arc::new(make_pipeline(cfg, sink))
}

/// Like [`build_pipeline`], but with a shortened conditional-forward receive
/// timeout so the timeout path can be driven in milliseconds instead of the
/// production 5 s.
fn build_pipeline_with_timeout(
    cfg: DnsConfig,
    sink: Option<Arc<DnsLogSink>>,
    recv_timeout: Duration,
) -> Arc<QueryPipeline> {
    let mut pipeline = make_pipeline(cfg, sink);
    pipeline.conditional_recv_timeout = recv_timeout;
    Arc::new(pipeline)
}

/// A `DnsConfig` whose only upstream is the stub at `addr`.
fn config_with_upstream(addr: SocketAddr) -> DnsConfig {
    DnsConfig {
        upstream_servers: vec![UpstreamDns {
            name: "stub".into(),
            address: addr.ip().to_string(),
            protocol: DnsProtocol::Udp,
            port: Some(addr.port()),
            tls_server_name: None,
        }],
        ..DnsConfig::default()
    }
}

/// Wire-encode a one-question query.
pub(super) fn query_bytes(id: u16, name: &str, rtype: RecordType) -> Vec<u8> {
    use hickory_proto::op::Query;
    use hickory_proto::rr::Name;

    let mut msg = Message::query();
    msg.metadata.id = id;
    msg.metadata.recursion_desired = true;
    msg.add_queries(vec![Query::query(
        Name::from_str_relaxed(name).expect("query name"),
        rtype,
    )]);
    msg.to_bytes().expect("encode query")
}

/// Run one query through the pipeline with a fresh [`ReplyCapture`] pair
/// and return the parsed response — `None` if the pipeline (correctly)
/// sent nothing. Dropping the capture closes the channel, so a zero-send
/// path yields `None` instead of hanging the receiver.
async fn ask(
    pipeline: &Arc<QueryPipeline>,
    packet: &[u8],
    client: ClientIdentity,
) -> Option<Message> {
    let (capture, mut rx) = ReplyCapture::channel();
    let reply: Arc<dyn DnsSocket> = Arc::new(capture);
    pipeline
        .handle(packet, src(), &reply, client, TransportProtocol::Udp)
        .await
        .expect("handle");
    drop(reply);
    rx.recv()
        .await
        .map(|frame| Message::from_bytes(&frame).expect("parse response"))
}

fn ip_client() -> ClientIdentity {
    ClientIdentity::Ip(src().ip())
}

/// What the stub upstream answers with.
enum StubAnswer {
    /// `NOERROR` with a single A record (TTL 60) for every A question.
    A(Ipv4Addr),
    /// `NXDOMAIN` carrying an SOA (TTL 300, MINIMUM 60) in the authority
    /// section, RFC 2308-style.
    NxDomain,
}

/// Spawn a tiny UDP responder driven by `answer`. Returns the bound
/// address; the task self-terminates when the runtime tears down.
async fn spawn_stub_upstream(answer: StubAnswer) -> SocketAddr {
    use hickory_proto::op::OpCode;
    use hickory_proto::rr::rdata::{A, SOA};
    use hickory_proto::rr::{Name, RData, Record};

    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
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
            match &answer {
                StubAnswer::A(ip) => {
                    for q in &request.queries {
                        if q.query_type() == RecordType::A {
                            response.add_answer(Record::from_rdata(
                                q.name().clone(),
                                60,
                                RData::A(A(*ip)),
                            ));
                        }
                    }
                }
                StubAnswer::NxDomain => {
                    response.metadata.response_code = ResponseCode::NXDomain;
                    let soa = SOA::new(
                        Name::from_str_relaxed("ns.com.").expect("mname"),
                        Name::from_str_relaxed("hostmaster.com.").expect("rname"),
                        1,
                        3600,
                        600,
                        86400,
                        60,
                    );
                    response.add_authority(Record::from_rdata(
                        Name::from_str_relaxed("com.").expect("soa name"),
                        300,
                        RData::SOA(soa),
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

async fn next_row(
    rx: &mut tokio::sync::mpsc::Receiver<wardnetd_data::repository::QueryLogRow>,
) -> wardnetd_data::repository::QueryLogRow {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("log row within 5s")
        .expect("sink open")
}

pub(super) fn manual_record(domain: &str, rt: DnsRecordType, value: &str) -> CustomDnsRecord {
    CustomDnsRecord {
        id: Uuid::new_v4(),
        zone_id: None,
        domain: domain.into(),
        record_type: rt,
        value: value.into(),
        ttl: 120,
        enabled: true,
        source: DnsRecordSource::Manual,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// ClientIdentity — attribution and the redacted Debug.
// ---------------------------------------------------------------------------

#[test]
fn debug_redacts_the_device_token() {
    let device = ClientIdentity::Device {
        device_id: Uuid::nil(),
        token: "wn-device-secret".into(),
    };
    let printed = format!("{device:?}");
    assert!(
        printed.contains("[REDACTED]"),
        "token field must render redacted: {printed}"
    );
    assert!(
        !printed.contains("wn-device-secret"),
        "the SNI device token must never reach a log line: {printed}"
    );

    let ip = ClientIdentity::Ip(IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!(format!("{ip:?}"), "Ip(127.0.0.1)");
}

#[tokio::test]
async fn device_identity_stamps_log_rows_without_a_snapshot_entry() {
    // The device snapshot is empty, so IP attribution would yield None —
    // the Device identity must carry the id into the log row on its own.
    let upstream = spawn_stub_upstream(StubAnswer::A(Ipv4Addr::new(93, 184, 216, 34))).await;
    let (sink, mut rx) = DnsLogSink::new();
    let pipeline = build_pipeline(config_with_upstream(upstream), Some(sink));

    let device_id = Uuid::new_v4();
    let response = ask(
        &pipeline,
        &query_bytes(0x1111, "example.com.", RecordType::A),
        ClientIdentity::Device {
            device_id,
            token: "wn-device-secret".into(),
        },
    )
    .await
    .expect("forwarded answer");

    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(response.answers.len(), 1);

    let row = next_row(&mut rx).await;
    assert_eq!(row.domain, "example.com");
    assert_eq!(
        row.device_id.as_deref(),
        Some(device_id.to_string()).as_deref()
    );
    assert_eq!(row.result, "forwarded");
}

// ---------------------------------------------------------------------------
// The zero-send exits: a transport must not assume a frame always comes.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn question_less_packet_is_dropped_without_a_reply() {
    let pipeline = build_pipeline(DnsConfig::default(), None);
    let mut msg = Message::query();
    msg.metadata.id = 0x2222;
    let packet = msg.to_bytes().expect("encode");

    let response = ask(&pipeline, &packet, ip_client()).await;
    assert!(
        response.is_none(),
        "a question-less query is dropped, not answered"
    );
}

#[tokio::test]
async fn unparseable_packet_errors_without_a_reply() {
    let pipeline = build_pipeline(DnsConfig::default(), None);
    let (capture, mut rx) = ReplyCapture::channel();
    let reply: Arc<dyn DnsSocket> = Arc::new(capture);

    let result = pipeline
        .handle(
            &[0xFF, 0x00, 0x01],
            src(),
            &reply,
            ip_client(),
            TransportProtocol::Udp,
        )
        .await;
    assert!(
        result.is_err(),
        "garbage bytes must surface as a parse error"
    );

    drop(reply);
    assert!(
        rx.recv().await.is_none(),
        "no frame may be sent for an unparseable packet"
    );
}

// ---------------------------------------------------------------------------
// Cache-hit reply path.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn repeat_query_is_served_from_the_response_cache() {
    let upstream = spawn_stub_upstream(StubAnswer::A(Ipv4Addr::new(93, 184, 216, 34))).await;
    let (sink, mut rx) = DnsLogSink::new();
    let pipeline = build_pipeline(config_with_upstream(upstream), Some(sink));

    let first = ask(
        &pipeline,
        &query_bytes(0x3333, "example.com.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("first answer");
    assert_eq!(next_row(&mut rx).await.result, "forwarded");

    // Same question, new transaction id: answered from cache, with the
    // id rewritten to the new query's.
    let second = ask(
        &pipeline,
        &query_bytes(0x4444, "example.com.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("cached answer");
    assert_eq!(next_row(&mut rx).await.result, "cache_hit");
    assert_eq!(second.metadata.id, 0x4444);
    assert_eq!(second.answers.len(), first.answers.len());
}

// ---------------------------------------------------------------------------
// Conditional forwarding — the pooled-socket relay path.
// ---------------------------------------------------------------------------

fn forwarding_rule(domain: &str, upstream: String) -> ConditionalForwardingRule {
    ConditionalForwardingRule {
        id: Uuid::new_v4(),
        domain: domain.into(),
        upstream,
        enabled: true,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn conditional_forwarding_relays_and_caches_the_upstream_answer() {
    // Default upstream is a black hole (127.0.0.1:1): an answer proves
    // the query went through the conditional rule, not the resolver.
    let cond_upstream = spawn_stub_upstream(StubAnswer::A(Ipv4Addr::new(203, 0, 113, 9))).await;
    let (sink, mut rx) = DnsLogSink::new();
    let pipeline = build_pipeline(
        config_with_upstream(SocketAddr::from(([127, 0, 0, 1], 1))),
        Some(sink),
    );
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![],
            vec![forwarding_rule("corp.example", cond_upstream.to_string())],
        )));

    let response = ask(
        &pipeline,
        &query_bytes(0x5555, "svc.corp.example.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("relayed answer");

    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(response.answers.len(), 1, "the stub's A record is relayed");

    let row = next_row(&mut rx).await;
    assert_eq!(row.result, "forwarded");
    assert_eq!(
        row.upstream.as_deref(),
        Some(cond_upstream.ip().to_string()).as_deref()
    );

    // The relayed answer is cached (TTL 60 from the stub): a repeat
    // query must not touch the upstream again.
    ask(
        &pipeline,
        &query_bytes(0x5556, "svc.corp.example.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("cached answer");
    assert_eq!(next_row(&mut rx).await.result, "cache_hit");
}

/// Issue #1184. A rule governs everything below its domain, but the cache is
/// consulted before the rule is applied, so a rule that lands while a
/// subdomain is already cached only takes effect if the eviction reaches the
/// whole subtree. The sticky case is exactly the one that sends an operator
/// to conditional forwarding in the first place: a negative answer whose TTL
/// comes from a parent zone's SOA.
#[tokio::test]
async fn a_new_rule_governs_already_cached_subdomains() {
    let default_upstream = spawn_stub_upstream(StubAnswer::NxDomain).await;
    let cond_upstream = spawn_stub_upstream(StubAnswer::A(Ipv4Addr::new(203, 0, 113, 42))).await;
    let (sink, mut rx) = DnsLogSink::new();
    let pipeline = build_pipeline(config_with_upstream(default_upstream), Some(sink));

    // The subdomain resolves NXDOMAIN, and the negative answer is cached.
    let first = ask(
        &pipeline,
        &query_bytes(0x7101, "mqtt.example.com.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("negative answer");
    assert_eq!(first.metadata.response_code, ResponseCode::NXDomain);
    assert_eq!(next_row(&mut rx).await.result, "negative");
    ask(
        &pipeline,
        &query_bytes(0x7102, "mqtt.example.com.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("cached negative");
    assert_eq!(next_row(&mut rx).await.result, "cache_hit");

    // The operator adds a rule on the parent domain: the runner swaps in the
    // new view and evicts the rule's subtree.
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![],
            vec![forwarding_rule("example.com", cond_upstream.to_string())],
        )));
    pipeline
        .cache
        .write()
        .await
        .invalidate_subtree("example.com");

    // The rule is live immediately — no manual cache flush, no TTL wait.
    let after = ask(
        &pipeline,
        &query_bytes(0x7103, "mqtt.example.com.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("conditionally forwarded answer");
    assert_eq!(after.metadata.response_code, ResponseCode::NoError);
    assert_eq!(after.answers.len(), 1, "the rule's upstream answered");
    let row = next_row(&mut rx).await;
    assert_eq!(row.result, "forwarded");
    assert_eq!(
        row.upstream.as_deref(),
        Some(cond_upstream.ip().to_string()).as_deref()
    );
}

#[tokio::test]
async fn conditional_forwarding_failure_returns_servfail() {
    // 127.0.0.1:1 is unbound — the connected socket's send/recv errors
    // fast (ICMP port unreachable), driving the ServFail fallback.
    let (sink, mut rx) = DnsLogSink::new();
    let pipeline = build_pipeline(DnsConfig::default(), Some(sink));
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![],
            vec![forwarding_rule("corp.example", "127.0.0.1:1".into())],
        )));

    let response = ask(
        &pipeline,
        &query_bytes(0x6666, "svc.corp.example.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("ServFail response");

    assert_eq!(response.metadata.response_code, ResponseCode::ServFail);
    assert_eq!(next_row(&mut rx).await.result, "upstream_error");
}

/// Encode a `NOERROR`/single-A response for `request`, mirroring what a real
/// upstream returns for an A question.
fn a_response(request: &Message, ip: Ipv4Addr) -> Vec<u8> {
    use hickory_proto::op::OpCode;
    use hickory_proto::rr::rdata::A;
    use hickory_proto::rr::{RData, Record};

    let mut response = Message::response(request.metadata.id, OpCode::Query);
    response.metadata.recursion_desired = true;
    response.metadata.recursion_available = true;
    response.add_queries(request.queries.clone());
    for q in &request.queries {
        if q.query_type() == RecordType::A {
            response.add_answer(Record::from_rdata(q.name().clone(), 60, RData::A(A(ip))));
        }
    }
    response.to_bytes().expect("encode stub response")
}

/// Spawn a conditional upstream that answers every query *twice*. The second
/// copy stays queued in the connected socket's receive buffer and becomes a
/// stale datagram for whichever pooled-socket borrower comes next — the same
/// hazard a late-after-timeout response creates, but deterministic.
async fn spawn_duplicating_upstream(ip: Ipv4Addr) -> SocketAddr {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("stub bind");
    let addr = socket.local_addr().expect("stub local_addr");
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            let Ok((n, peer)) = socket.recv_from(&mut buf).await else {
                break;
            };
            let Ok(request) = Message::from_bytes(&buf[..n]) else {
                continue;
            };
            let bytes = a_response(&request, ip);
            let _ = socket.send_to(&bytes, peer).await;
            let _ = socket.send_to(&bytes, peer).await;
        }
    });
    addr
}

/// Spawn a conditional upstream that stalls its response to the *first* query
/// past `delay` (simulating an upstream slower than the receive timeout), then
/// answers every later query promptly.
async fn spawn_slow_first_upstream(ip: Ipv4Addr, delay: Duration) -> SocketAddr {
    use std::sync::atomic::{AtomicBool, Ordering};

    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("stub bind");
    let addr = socket.local_addr().expect("stub local_addr");
    tokio::spawn(async move {
        let first = AtomicBool::new(true);
        let mut buf = vec![0u8; 4096];
        loop {
            let Ok((n, peer)) = socket.recv_from(&mut buf).await else {
                break;
            };
            let Ok(request) = Message::from_bytes(&buf[..n]) else {
                continue;
            };
            let bytes = a_response(&request, ip);
            if first.swap(false, Ordering::Relaxed) {
                tokio::time::sleep(delay).await;
            }
            let _ = socket.send_to(&bytes, peer).await;
        }
    });
    addr
}

/// Spawn a conditional upstream that answers every query with the same fixed
/// bytes regardless of content — used to feed the forwarder a datagram hickory
/// cannot decode.
async fn spawn_fixed_bytes_upstream(bytes: Vec<u8>) -> SocketAddr {
    let socket = tokio::net::UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("stub bind");
    let addr = socket.local_addr().expect("stub local_addr");
    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            let Ok((_, peer)) = socket.recv_from(&mut buf).await else {
                break;
            };
            let _ = socket.send_to(&bytes, peer).await;
        }
    });
    addr
}

/// A stale datagram left queued on a pooled socket by an earlier query must
/// not turn the *next* query into a SERVFAIL: the reuse path skips it and
/// keeps reading until it gets its own answer.
#[tokio::test]
async fn conditional_forward_skips_a_stale_pooled_datagram() {
    let cond_upstream = spawn_duplicating_upstream(Ipv4Addr::new(203, 0, 113, 9)).await;
    let pipeline = build_pipeline(
        config_with_upstream(SocketAddr::from(([127, 0, 0, 1], 1))),
        None,
    );
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![],
            vec![forwarding_rule("corp.example", cond_upstream.to_string())],
        )));

    // Query A checks a socket out of the (empty) pool, gets its answer, and
    // returns the socket with A's duplicate response still queued on it.
    let a = ask(
        &pipeline,
        &query_bytes(0xAAAA, "a.corp.example.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("query A answer");
    assert_eq!(a.metadata.response_code, ResponseCode::NoError);

    // Query B reuses that same pooled socket — A's stale datagram is at the
    // head of its receive buffer. It must be skipped, not served as B's answer
    // and not surfaced as a SERVFAIL.
    let b = ask(
        &pipeline,
        &query_bytes(0xBBBB, "b.corp.example.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("query B answer");
    assert_eq!(
        b.metadata.response_code,
        ResponseCode::NoError,
        "B must get its own answer, not a SERVFAIL from A's stale datagram"
    );
    assert_eq!(b.metadata.id, 0xBBBB, "the answer is B's own, not A's");
    assert_eq!(
        b.queries.first().map(|q| q.name().to_ascii()),
        Some("b.corp.example.".to_string())
    );
}

/// A query that times out must drop its socket rather than pool it, so the
/// late response can't queue on a reused socket and SERVFAIL the next query.
#[tokio::test]
async fn conditional_forward_drops_the_socket_on_timeout() {
    // The stub stalls A's reply to 1.2 s, well past the 500 ms receive timeout,
    // so A reliably times out; B, on a fresh socket, has the full 500 ms for a
    // sub-millisecond loopback round trip — ample headroom on a loaded runner.
    let cond_upstream =
        spawn_slow_first_upstream(Ipv4Addr::new(203, 0, 113, 9), Duration::from_millis(1200)).await;
    let pipeline = build_pipeline_with_timeout(
        config_with_upstream(SocketAddr::from(([127, 0, 0, 1], 1))),
        None,
        Duration::from_millis(500),
    );
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![],
            vec![forwarding_rule("corp.example", cond_upstream.to_string())],
        )));

    // Query A's upstream is slower than the receive timeout, so A times out.
    let a = ask(
        &pipeline,
        &query_bytes(0xAAAA, "a.corp.example.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("query A response");
    assert_eq!(a.metadata.response_code, ResponseCode::ServFail);

    // Let A's late response land (it fires at ~1.2 s). If the timed-out socket
    // had been pooled, this would now sit queued on it, at the head of the
    // buffer, waiting to poison the next borrower.
    tokio::time::sleep(Duration::from_millis(1200)).await;

    // Query B must still get its own answer — it can only do so on a socket
    // free of A's stale datagram.
    let b = ask(
        &pipeline,
        &query_bytes(0xBBBB, "b.corp.example.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("query B answer");
    assert_eq!(
        b.metadata.response_code,
        ResponseCode::NoError,
        "B must not inherit a SERVFAIL from A's timed-out socket"
    );
    assert_eq!(b.metadata.id, 0xBBBB, "the answer is B's own, not A's");
}

/// A first response the resolver can't decode is still relayed to the client
/// raw (its own resolver may parse it), not skipped into a SERVFAIL.
#[tokio::test]
async fn conditional_forward_relays_an_undecodable_first_response_raw() {
    // Three bytes is too short for a DNS header, so `Message::from_bytes` fails.
    let undecodable = vec![0x00u8, 0x01, 0x02];
    let cond_upstream = spawn_fixed_bytes_upstream(undecodable.clone()).await;
    let pipeline = build_pipeline(
        config_with_upstream(SocketAddr::from(([127, 0, 0, 1], 1))),
        None,
    );
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![],
            vec![forwarding_rule("corp.example", cond_upstream.to_string())],
        )));

    // Capture the raw frame directly — `ask` would try to parse it and panic.
    let (capture, mut rx) = ReplyCapture::channel();
    let reply: Arc<dyn DnsSocket> = Arc::new(capture);
    pipeline
        .handle(
            &query_bytes(0xABCD, "svc.corp.example.", RecordType::A),
            src(),
            &reply,
            ip_client(),
            TransportProtocol::Udp,
        )
        .await
        .expect("handle");
    drop(reply);

    let frame = rx.recv().await.expect("raw relayed frame");
    assert_eq!(
        frame, undecodable,
        "an undecodable first response is relayed byte-for-byte, not dropped"
    );
}

// ---------------------------------------------------------------------------
// Tunnel dispatch: a routing-snapshot entry whose tunnel can't be
// resolved into a forwarder must ServFail, not fall back to the default
// resolver (that would leak the query outside the tunnel).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unresolvable_tunnel_forwarder_returns_servfail() {
    let (sink, mut rx) = DnsLogSink::new();
    // Stub upstream would answer if the query leaked to the default path.
    let upstream = spawn_stub_upstream(StubAnswer::A(Ipv4Addr::new(93, 184, 216, 34))).await;
    let pipeline = build_pipeline(config_with_upstream(upstream), Some(sink));
    // The stub tunnel repo knows no tunnels, so forwarder build fails.
    pipeline.routing_snapshot.store(Arc::new(HashMap::from([(
        src().ip(),
        UpstreamId::Tunnel(Uuid::new_v4()),
    )])));

    let response = ask(
        &pipeline,
        &query_bytes(0x7777, "example.com.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("ServFail response");

    assert_eq!(
        response.metadata.response_code,
        ResponseCode::ServFail,
        "an unresolvable tunnel must fail closed, never leak to the default upstream"
    );
    assert!(response.answers.is_empty());
    assert_eq!(next_row(&mut rx).await.result, "upstream_error");
}

// ---------------------------------------------------------------------------
// Per-device upstream selection for device-authenticated clients (#923):
// a DoT client's upstream keys on its device identity through the
// device-keyed snapshot, never on the transport source IP — while roaming
// that IP is the relay's loopback, which may collide with (or miss) the
// IP-keyed map.
// ---------------------------------------------------------------------------

fn device_client(device_id: Uuid) -> ClientIdentity {
    ClientIdentity::Device {
        device_id,
        token: "aaaaaaaaaaaaaaaa".to_owned(),
    }
}

/// The device-keyed binding takes precedence over the transport IP's
/// entry: a device whose own binding is `Default` resolves via the
/// default upstream even when the source IP maps to a (bogus) tunnel
/// that would fail closed.
#[tokio::test]
async fn device_authenticated_client_prefers_device_binding_over_ip_map() {
    let upstream = spawn_stub_upstream(StubAnswer::A(Ipv4Addr::new(93, 184, 216, 34))).await;
    let pipeline = build_pipeline(config_with_upstream(upstream), None);
    // If this entry won, the query would fail closed (ServFail): the stub
    // tunnel repo cannot resolve a forwarder for any tunnel.
    pipeline.routing_snapshot.store(Arc::new(HashMap::from([(
        src().ip(),
        UpstreamId::Tunnel(Uuid::new_v4()),
    )])));
    let device_id = Uuid::new_v4();
    pipeline
        .device_routing_snapshot
        .store(Arc::new(HashMap::from([(device_id, UpstreamId::Default)])));

    let response = ask(
        &pipeline,
        &query_bytes(0x2923, "example.com.", RecordType::A),
        device_client(device_id),
    )
    .await
    .expect("answer from the default upstream");

    assert_eq!(
        response.metadata.response_code,
        ResponseCode::NoError,
        "the device's own binding must outrank the transport IP's tunnel binding"
    );
    assert_eq!(response.answers.len(), 1);
}

/// On a device-map miss the pipeline falls back to the applied-state IP
/// map, so an on-LAN `DoT` client keeps the binding the kernel is actually
/// enforcing even while the device map is stale or still building. (A
/// roaming client's src is the relay loopback, which always misses.)
#[tokio::test]
async fn device_authenticated_client_falls_back_to_ip_map_on_device_map_miss() {
    let (sink, mut rx) = DnsLogSink::new();
    // Would answer if the fallback failed to fire and the query went to
    // the default path.
    let upstream = spawn_stub_upstream(StubAnswer::A(Ipv4Addr::new(93, 184, 216, 34))).await;
    let pipeline = build_pipeline(config_with_upstream(upstream), Some(sink));
    pipeline.routing_snapshot.store(Arc::new(HashMap::from([(
        src().ip(),
        UpstreamId::Tunnel(Uuid::new_v4()),
    )])));

    let response = ask(
        &pipeline,
        &query_bytes(0x2924, "example.com.", RecordType::A),
        device_client(Uuid::new_v4()),
    )
    .await
    .expect("ServFail response");

    assert_eq!(
        response.metadata.response_code,
        ResponseCode::ServFail,
        "the IP-keyed tunnel binding must apply on a device-map miss (and fail closed here)"
    );
    assert_eq!(next_row(&mut rx).await.result, "upstream_error");
}

/// A device-keyed `Tunnel(_)` binding is honored — and fails closed like
/// the IP-keyed path when the forwarder can't be built, never leaking the
/// roaming device's query to the default upstream.
#[tokio::test]
async fn device_authenticated_client_tunnel_binding_fails_closed() {
    let (sink, mut rx) = DnsLogSink::new();
    // Would answer if the query leaked to the default path.
    let upstream = spawn_stub_upstream(StubAnswer::A(Ipv4Addr::new(93, 184, 216, 34))).await;
    let pipeline = build_pipeline(config_with_upstream(upstream), Some(sink));
    let device_id = Uuid::new_v4();
    pipeline
        .device_routing_snapshot
        .store(Arc::new(HashMap::from([(
            device_id,
            UpstreamId::Tunnel(Uuid::new_v4()),
        )])));

    let response = ask(
        &pipeline,
        &query_bytes(0x3923, "example.com.", RecordType::A),
        device_client(device_id),
    )
    .await
    .expect("ServFail response");

    assert_eq!(
        response.metadata.response_code,
        ResponseCode::ServFail,
        "a device's unresolvable tunnel binding must fail closed, not leak to the default upstream"
    );
    assert!(response.answers.is_empty());
    let row = next_row(&mut rx).await;
    assert_eq!(row.result, "upstream_error");
    assert_eq!(
        row.device_id.as_deref(),
        Some(device_id.to_string()).as_deref(),
        "the log row keeps the device attribution"
    );
}

// ---------------------------------------------------------------------------
// Rebinding protection on the forwarded path.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rebinding_protection_rejects_private_answers_for_external_domains() {
    let upstream = spawn_stub_upstream(StubAnswer::A(Ipv4Addr::new(192, 168, 1, 10))).await;
    let (sink, mut rx) = DnsLogSink::new();
    let cfg = DnsConfig {
        rebinding_protection: true,
        ..config_with_upstream(upstream)
    };
    let pipeline = build_pipeline(cfg, Some(sink));

    let response = ask(
        &pipeline,
        &query_bytes(0x8888, "example.com.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("NODATA response");

    // Empty NOERROR, not NXDOMAIN: the name exists, the answer is refused.
    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert!(
        response.answers.is_empty(),
        "the private-IP answer must not be relayed"
    );
    assert_eq!(next_row(&mut rx).await.result, "rebinding_blocked");
}

// ---------------------------------------------------------------------------
// Negative relaying (RFC 2308) on the forwarded path.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn upstream_nxdomain_is_relayed_with_the_negative_ttl() {
    let upstream = spawn_stub_upstream(StubAnswer::NxDomain).await;
    let (sink, mut rx) = DnsLogSink::new();
    let pipeline = build_pipeline(config_with_upstream(upstream), Some(sink));

    let response = ask(
        &pipeline,
        &query_bytes(0x9999, "nosuch.example.", RecordType::A),
        ip_client(),
    )
    .await
    .expect("relayed negative");

    assert_eq!(
        response.metadata.response_code,
        ResponseCode::NXDomain,
        "a negative answer is a valid resolution, never ServFail"
    );
    assert!(response.answers.is_empty());
    assert!(
        !response.authorities.is_empty(),
        "the SOA must be relayed so clients can negatively cache"
    );
    assert_eq!(next_row(&mut rx).await.result, "negative");
}

// ---------------------------------------------------------------------------
// Authoritative answers beyond A/AAAA: TXT/MX/SRV rdata building, the
// ANY fan-out, and the skip-malformed-record branch.
// ---------------------------------------------------------------------------

fn records_fixture() -> Vec<CustomDnsRecord> {
    vec![
        manual_record("note.lab", DnsRecordType::Txt, "hello world"),
        manual_record("corp.lab", DnsRecordType::Mx, "10 mail.corp.lab"),
        manual_record("svc.lab", DnsRecordType::Srv, "1 2 8080 target.lab"),
        // Malformed on purpose: MX needs "<priority> <exchange>".
        manual_record("broken.lab", DnsRecordType::Mx, "banana"),
    ]
}

#[tokio::test]
async fn authoritative_txt_mx_and_srv_records_resolve() {
    let pipeline = build_pipeline(DnsConfig::default(), None);
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            records_fixture(),
            vec![],
        )));

    for (name, rtype) in [
        ("note.lab.", RecordType::TXT),
        ("corp.lab.", RecordType::MX),
        ("svc.lab.", RecordType::SRV),
    ] {
        let response = ask(&pipeline, &query_bytes(0xA000, name, rtype), ip_client())
            .await
            .expect("authoritative answer");
        assert_eq!(response.metadata.response_code, ResponseCode::NoError);
        assert!(
            response.metadata.authoritative,
            "{name} must carry the AA bit"
        );
        assert_eq!(response.answers.len(), 1, "{name} {rtype:?} answers");
    }
}

#[tokio::test]
async fn authoritative_any_query_returns_all_records_for_the_name() {
    let pipeline = build_pipeline(DnsConfig::default(), None);
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            records_fixture(),
            vec![],
        )));

    let response = ask(
        &pipeline,
        &query_bytes(0xB000, "note.lab.", RecordType::ANY),
        ip_client(),
    )
    .await
    .expect("ANY answer");
    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(
        response.answers.len(),
        1,
        "note.lab has exactly its TXT record"
    );
}

#[tokio::test]
async fn malformed_authoritative_record_is_skipped_not_served() {
    let pipeline = build_pipeline(DnsConfig::default(), None);
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            records_fixture(),
            vec![],
        )));

    // The broken MX value fails rdata building; the record is skipped
    // and the client sees an empty NOERROR rather than garbage.
    let response = ask(
        &pipeline,
        &query_bytes(0xC000, "broken.lab.", RecordType::MX),
        ip_client(),
    )
    .await
    .expect("authoritative answer");
    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert!(
        response.answers.is_empty(),
        "the malformed record must be dropped"
    );
}

// ---------------------------------------------------------------------------
// Reverse PTR for private ranges: the hostname answer, the authoritative
// NXDOMAIN for an unknown private address, the rate-limiter exemption that
// breaks the RFC1918 PTR flood, and the public-IP / conditional-rule
// fall-through paths.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reverse_ptr_for_a_known_private_ip_answers_with_the_hostname() {
    use hickory_proto::rr::RData;

    let (sink, mut rx) = DnsLogSink::new();
    let pipeline = build_pipeline(DnsConfig::default(), Some(sink));
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![manual_record(
                "laptop.lan",
                DnsRecordType::A,
                "192.168.100.5",
            )],
            vec![],
        )));

    let response = ask(
        &pipeline,
        &query_bytes(0xD000, "5.100.168.192.in-addr.arpa.", RecordType::PTR),
        ip_client(),
    )
    .await
    .expect("authoritative PTR answer");

    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert!(response.metadata.authoritative, "PTR must carry the AA bit");
    assert_eq!(response.answers.len(), 1);
    match &response.answers[0].data {
        RData::PTR(ptr) => assert_eq!(ptr.0.to_string(), "laptop.lan."),
        other => panic!("expected a PTR record, got {other:?}"),
    }
    assert_eq!(next_row(&mut rx).await.result, "authoritative");
}

#[tokio::test]
async fn reverse_ptr_for_an_unknown_private_ip_is_authoritative_nxdomain() {
    let (sink, mut rx) = DnsLogSink::new();
    // A black-hole upstream: if the query leaked to forwarding it would hang,
    // not answer — so a prompt NXDOMAIN proves it was answered locally.
    let pipeline = build_pipeline(
        config_with_upstream(SocketAddr::from(([127, 0, 0, 1], 1))),
        Some(sink),
    );

    let response = ask(
        &pipeline,
        &query_bytes(0xD001, "9.100.168.192.in-addr.arpa.", RecordType::PTR),
        ip_client(),
    )
    .await
    .expect("authoritative NXDOMAIN");

    assert_eq!(response.metadata.response_code, ResponseCode::NXDomain);
    assert!(
        response.metadata.authoritative,
        "NXDOMAIN must carry the AA bit"
    );
    assert!(response.answers.is_empty());
    assert!(
        !response.authorities.is_empty(),
        "the synthetic SOA must ride along so the negative is cacheable"
    );
    assert_eq!(next_row(&mut rx).await.result, "authoritative_nxdomain");
}

#[tokio::test]
async fn reverse_ptr_for_private_ranges_is_never_rate_limited() {
    // Budget of one query/sec: the storm the mesh nodes generate would trip
    // this instantly under the old ordering. With the limiter scoped to the
    // upstream path, every private PTR is answered locally and none is REFUSED.
    let cfg = DnsConfig {
        rate_limit_per_second: 1,
        ..config_with_upstream(SocketAddr::from(([127, 0, 0, 1], 1)))
    };
    let pipeline = build_pipeline(cfg, None);

    for i in 0..10u8 {
        let name = format!("{i}.100.168.192.in-addr.arpa.");
        let response = ask(
            &pipeline,
            &query_bytes(0xD100 + u16::from(i), &name, RecordType::PTR),
            ip_client(),
        )
        .await
        .expect("PTR answered locally");
        assert_ne!(
            response.metadata.response_code,
            ResponseCode::Refused,
            "private PTR #{i} must never be rate-limited"
        );
        assert_eq!(response.metadata.response_code, ResponseCode::NXDomain);
    }
}

#[tokio::test]
async fn reverse_ptr_for_a_public_ip_falls_through_to_forwarding() {
    let upstream = spawn_stub_upstream(StubAnswer::NxDomain).await;
    let (sink, mut rx) = DnsLogSink::new();
    let pipeline = build_pipeline(config_with_upstream(upstream), Some(sink));

    // 8.8.8.8 is public — not our reverse space, so the query is forwarded.
    let response = ask(
        &pipeline,
        &query_bytes(0xD200, "8.8.8.8.in-addr.arpa.", RecordType::PTR),
        ip_client(),
    )
    .await
    .expect("forwarded negative");

    assert!(
        !response.metadata.authoritative,
        "a public-range PTR must not be answered authoritatively"
    );
    assert_eq!(next_row(&mut rx).await.result, "negative");
}

#[tokio::test]
async fn reverse_ptr_conditional_forwarding_rule_wins_over_local_authority() {
    let upstream = spawn_stub_upstream(StubAnswer::NxDomain).await;
    let (sink, mut rx) = DnsLogSink::new();
    // Default upstream is a black hole; only the conditional rule can answer.
    let pipeline = build_pipeline(
        config_with_upstream(SocketAddr::from(([127, 0, 0, 1], 1))),
        Some(sink),
    );
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![],
            vec![forwarding_rule(
                "168.192.in-addr.arpa",
                upstream.to_string(),
            )],
        )));

    let response = ask(
        &pipeline,
        &query_bytes(0xD300, "5.100.168.192.in-addr.arpa.", RecordType::PTR),
        ip_client(),
    )
    .await
    .expect("conditionally forwarded answer");

    // The rule routed it to the stub (which we reached) rather than the local
    // synthetic NXDOMAIN, so the answer is non-authoritative and attributed to
    // the conditional upstream.
    assert!(!response.metadata.authoritative);
    let row = next_row(&mut rx).await;
    assert_eq!(
        row.upstream.as_deref(),
        Some(upstream.ip().to_string()).as_deref(),
        "the reverse query must go to the conditional upstream"
    );
}

#[tokio::test]
async fn reverse_any_query_for_a_private_ip_is_answered_locally() {
    use hickory_proto::rr::RData;

    let (sink, mut rx) = DnsLogSink::new();
    // Black-hole upstream: a prompt local answer proves it did not forward.
    let pipeline = build_pipeline(
        config_with_upstream(SocketAddr::from(([127, 0, 0, 1], 1))),
        Some(sink),
    );
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![manual_record(
                "printer.lan",
                DnsRecordType::A,
                "192.168.100.7",
            )],
            vec![],
        )));

    // An ANY query for a private reverse name must be answered locally too,
    // otherwise the "never leak private reverse upstream" guarantee has a hole.
    let response = ask(
        &pipeline,
        &query_bytes(0xD400, "7.100.168.192.in-addr.arpa.", RecordType::ANY),
        ip_client(),
    )
    .await
    .expect("authoritative ANY answer");

    assert!(
        response.metadata.authoritative,
        "ANY reverse must be answered locally, not forwarded"
    );
    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert_eq!(response.answers.len(), 1);
    match &response.answers[0].data {
        RData::PTR(ptr) => assert_eq!(ptr.0.to_string(), "printer.lan."),
        other => panic!("expected a PTR record, got {other:?}"),
    }
    assert_eq!(next_row(&mut rx).await.result, "authoritative");
}

#[tokio::test]
async fn reverse_ptr_with_only_unrenderable_targets_is_nodata_not_nxdomain() {
    let (sink, mut rx) = DnsLogSink::new();
    let pipeline = build_pipeline(
        config_with_upstream(SocketAddr::from(([127, 0, 0, 1], 1))),
        Some(sink),
    );
    // A 64-octet label exceeds the DNS limit, so the forward record is indexed
    // for the reverse (its value is a private IP) but its name can't be rendered
    // back into a PTR target.
    let bad_label = "a".repeat(64);
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![manual_record(
                &format!("{bad_label}.lan"),
                DnsRecordType::A,
                "192.168.100.8",
            )],
            vec![],
        )));

    let response = ask(
        &pipeline,
        &query_bytes(0xD500, "8.100.168.192.in-addr.arpa.", RecordType::PTR),
        ip_client(),
    )
    .await
    .expect("authoritative NODATA");

    // The address exists (a record names it) but no PTR could be rendered, so
    // this is NODATA — NoError with the SOA for negative caching — not NXDOMAIN,
    // which would wrongly assert the name does not exist.
    assert_eq!(response.metadata.response_code, ResponseCode::NoError);
    assert!(response.answers.is_empty());
    assert!(
        !response.authorities.is_empty(),
        "the SOA must ride along so the negative is cacheable"
    );
    assert_eq!(next_row(&mut rx).await.result, "authoritative_nodata");
}
