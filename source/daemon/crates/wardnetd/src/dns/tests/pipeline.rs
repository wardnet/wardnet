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
pub(super) fn build_pipeline(cfg: DnsConfig, sink: Option<Arc<DnsLogSink>>) -> Arc<QueryPipeline> {
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
        Arc::new(ArcSwap::from_pointee(HashMap::new())),
        Arc::new(ArcSwap::from_pointee(HashMap::new())),
        stub_tunnel_repo(),
    );
    pipeline.log_sink = sink;
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
