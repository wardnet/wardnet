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

#[tokio::test]
async fn noop_server_cache_hit_rate_returns_zero() {
    let server = NoopDnsServer;
    assert!(server.cache_hit_rate().await.abs() < f64::EPSILON);
}

#[tokio::test]
async fn noop_server_update_config_is_noop() {
    let server = NoopDnsServer;
    // Should not panic or block.
    server.update_config(DnsConfig::default()).await;
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
    server
        .update_config(DnsConfig {
            cache_size: 5_000,
            cache_ttl_max_secs: 3_600,
            ..DnsConfig::default()
        })
        .await;

    // Update while running should also succeed.
    server.start().await.unwrap();

    server
        .update_config(DnsConfig {
            cache_size: 20_000,
            ..DnsConfig::default()
        })
        .await;

    server.stop().await.unwrap();
}

#[tokio::test]
async fn build_resolver_with_empty_upstreams_uses_cloudflare() {
    // When upstream_servers is empty, the server should still start
    // successfully because `build_resolver` falls back to Cloudflare.
    let config = DnsConfig {
        upstream_servers: vec![],
        ..DnsConfig::default()
    };

    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());

    server.start().await.unwrap();
    assert!(
        server.is_running(),
        "server should start even with empty upstreams (Cloudflare fallback)"
    );

    server.stop().await.unwrap();
}

// ---------------------------------------------------------------------------
// MockDnsSocket
// ---------------------------------------------------------------------------

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use hickory_proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
use hickory_proto::rr::{Name, RecordType};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use tokio::sync::Mutex;

use crate::dns::server::DnsSocket;

/// Mock socket that stores sent packets and returns pre-loaded received packets.
struct MockDnsSocket {
    incoming: Mutex<VecDeque<(Vec<u8>, SocketAddr)>>,
    outgoing: Mutex<Vec<(Vec<u8>, SocketAddr)>>,
    notify: tokio::sync::Notify,
}

impl MockDnsSocket {
    fn new() -> Self {
        Self {
            incoming: Mutex::new(VecDeque::new()),
            outgoing: Mutex::new(Vec::new()),
            notify: tokio::sync::Notify::new(),
        }
    }

    async fn push_packet(&self, data: Vec<u8>, src: SocketAddr) {
        self.incoming.lock().await.push_back((data, src));
        self.notify.notify_one();
    }

    async fn sent_packets(&self) -> Vec<(Vec<u8>, SocketAddr)> {
        self.outgoing.lock().await.clone()
    }
}

#[async_trait]
impl DnsSocket for MockDnsSocket {
    async fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        loop {
            {
                let mut queue = self.incoming.lock().await;
                if let Some((data, addr)) = queue.pop_front() {
                    let len = data.len().min(buf.len());
                    buf[..len].copy_from_slice(&data[..len]);
                    return Ok((len, addr));
                }
            }
            self.notify.notified().await;
        }
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> std::io::Result<usize> {
        let data = buf.to_vec();
        let len = data.len();
        self.outgoing.lock().await.push((data, target));
        Ok(len)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal DNS query packet for the given domain and record type.
fn build_dns_query(domain: &str, id: u16) -> Vec<u8> {
    let mut msg = Message::new();
    msg.set_id(id);
    msg.set_message_type(MessageType::Query);
    msg.set_op_code(OpCode::Query);
    msg.set_recursion_desired(true);

    let name = Name::from_ascii(domain).expect("invalid domain name");
    let mut query = Query::new();
    query.set_name(name);
    query.set_query_type(RecordType::A);
    msg.add_query(query);

    msg.to_bytes().expect("failed to serialize DNS query")
}

fn client_addr() -> SocketAddr {
    SocketAddr::from(([192, 168, 1, 100], 12345))
}

/// Default config pointing at an unreachable upstream so forwarding fails fast.
fn test_config() -> DnsConfig {
    use wardnet_types::dns::{DnsProtocol, UpstreamDns};
    DnsConfig {
        enabled: true,
        upstream_servers: vec![UpstreamDns {
            address: "192.0.2.1".to_string(), // TEST-NET, unreachable
            name: "test".to_string(),
            port: Some(53),
            protocol: DnsProtocol::Udp,
        }],
        cache_size: 1000,
        cache_ttl_min_secs: 0,
        cache_ttl_max_secs: 86400,
        ..DnsConfig::default()
    }
}

/// Wait until the mock socket has at least `expected` outgoing packets.
///
/// The upstream resolver may take several seconds to time out when pointing
/// at an unreachable address, so we poll for up to 30 seconds.
async fn wait_for_packets(socket: &MockDnsSocket, expected: usize) {
    for _ in 0..300 {
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        if socket.sent_packets().await.len() >= expected {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Query handling tests (exercises server_loop / handle_query / build_resolver)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_responds_to_query() {
    let socket = Arc::new(MockDnsSocket::new());
    let server =
        UdpDnsServer::with_socket(test_config(), Arc::clone(&socket) as Arc<dyn DnsSocket>);

    server.start().await.unwrap();
    assert!(server.is_running());

    let query_id: u16 = 0xABCD;
    let packet = build_dns_query("example.com.", query_id);
    socket.push_packet(packet, client_addr()).await;

    wait_for_packets(&socket, 1).await;

    let sent = socket.sent_packets().await;
    assert!(!sent.is_empty(), "server should send a response");

    let (resp_bytes, resp_addr) = &sent[0];
    assert_eq!(
        *resp_addr,
        client_addr(),
        "response should go back to client"
    );

    let resp = Message::from_bytes(resp_bytes).expect("response should be valid DNS");
    assert_eq!(resp.id(), query_id, "response ID must match query ID");
    assert_eq!(resp.message_type(), MessageType::Response);

    // Upstream is unreachable, so we expect SERVFAIL (or NoError if somehow resolved).
    let code = resp.response_code();
    assert!(
        code == ResponseCode::ServFail || code == ResponseCode::NoError,
        "expected ServFail or NoError, got {code:?}"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn server_handles_malformed_packet() {
    let socket = Arc::new(MockDnsSocket::new());
    let server =
        UdpDnsServer::with_socket(test_config(), Arc::clone(&socket) as Arc<dyn DnsSocket>);

    server.start().await.unwrap();

    // Push garbage bytes -- not a valid DNS message.
    let garbage = vec![0xFF, 0xFE, 0x00, 0x01, 0xDE, 0xAD];
    socket.push_packet(garbage, client_addr()).await;

    // Give the server time to process (and not crash).
    tokio::time::sleep(Duration::from_millis(500)).await;
    tokio::task::yield_now().await;

    // Server should still be running.
    assert!(
        server.is_running(),
        "server must not crash on malformed input"
    );

    // Push a valid query after the garbage to prove the server is still alive.
    let packet = build_dns_query("after-garbage.test.", 0x1234);
    socket.push_packet(packet, client_addr()).await;

    wait_for_packets(&socket, 1).await;

    let sent = socket.sent_packets().await;
    // We may have 0 or 1 responses for the garbage (likely 0), plus 1 for the
    // valid query. Just verify at least 1 valid response exists.
    let valid_responses: Vec<_> = sent
        .iter()
        .filter_map(|(bytes, _)| Message::from_bytes(bytes).ok())
        .filter(|m| m.message_type() == MessageType::Response)
        .collect();
    assert!(
        !valid_responses.is_empty(),
        "server should respond to valid queries after receiving garbage"
    );

    server.stop().await.unwrap();
}

#[tokio::test]
async fn server_processes_multiple_queries() {
    let socket = Arc::new(MockDnsSocket::new());
    let server =
        UdpDnsServer::with_socket(test_config(), Arc::clone(&socket) as Arc<dyn DnsSocket>);

    server.start().await.unwrap();

    let domains = [
        ("alpha.test.", 0x0001_u16),
        ("beta.test.", 0x0002),
        ("gamma.test.", 0x0003),
    ];

    for (domain, id) in &domains {
        let packet = build_dns_query(domain, *id);
        socket.push_packet(packet, client_addr()).await;
    }

    wait_for_packets(&socket, 3).await;

    let sent = socket.sent_packets().await;
    assert!(
        sent.len() >= 3,
        "expected at least 3 responses, got {}",
        sent.len()
    );

    // Parse all responses and collect their IDs.
    let response_ids: Vec<u16> = sent
        .iter()
        .filter_map(|(bytes, _)| Message::from_bytes(bytes).ok())
        .filter(|m| m.message_type() == MessageType::Response)
        .map(|m| m.id())
        .collect();

    // All three query IDs should appear in responses (order may differ due to
    // async spawning).
    for (_, id) in &domains {
        assert!(
            response_ids.contains(id),
            "missing response for query ID {id:#06X}"
        );
    }

    server.stop().await.unwrap();
}

#[tokio::test]
async fn server_cache_hit() {
    let socket = Arc::new(MockDnsSocket::new());
    let server =
        UdpDnsServer::with_socket(test_config(), Arc::clone(&socket) as Arc<dyn DnsSocket>);

    server.start().await.unwrap();

    // First query -- will forward upstream (and likely get SERVFAIL).
    let packet1 = build_dns_query("cached.test.", 0x1111);
    socket.push_packet(packet1, client_addr()).await;
    wait_for_packets(&socket, 1).await;

    let sent_after_first = socket.sent_packets().await;
    assert!(!sent_after_first.is_empty(), "should get first response");

    let first_resp =
        Message::from_bytes(&sent_after_first[0].0).expect("first response should parse");
    let first_code = first_resp.response_code();

    // Second identical query.
    let packet2 = build_dns_query("cached.test.", 0x2222);
    socket.push_packet(packet2, client_addr()).await;
    wait_for_packets(&socket, 2).await;

    let sent_after_second = socket.sent_packets().await;
    assert!(
        sent_after_second.len() >= 2,
        "should get second response, got {}",
        sent_after_second.len()
    );

    let second_resp =
        Message::from_bytes(&sent_after_second[1].0).expect("second response should parse");

    // Second response must have its own query ID.
    assert_eq!(second_resp.id(), 0x2222);
    assert_eq!(second_resp.message_type(), MessageType::Response);

    // If the first response was NoError (upstream actually responded), the
    // second should also be NoError from cache. If SERVFAIL, the second will
    // also be SERVFAIL (SERVFAIL responses are not cached).
    if first_code == ResponseCode::NoError {
        assert_eq!(
            second_resp.response_code(),
            ResponseCode::NoError,
            "cached response should also be NoError"
        );
    } else {
        // SERVFAIL is not cached, so second query also forwards and likely
        // also returns SERVFAIL.
        assert_eq!(
            second_resp.response_code(),
            ResponseCode::ServFail,
            "uncached SERVFAIL should forward again"
        );
    }

    server.stop().await.unwrap();
}

// ---------------------------------------------------------------------------
// Mock upstream DNS server — runs a real UDP listener that answers queries,
// so we can exercise the successful Ok(lookup) path in handle_query.
// ---------------------------------------------------------------------------

use hickory_proto::rr::{RData, Record, rdata::A};
use tokio::net::UdpSocket;

/// Start a minimal DNS responder on loopback that answers every A query
/// with 127.0.0.42 (TTL 60). Returns the bound `SocketAddr`.
async fn start_mock_upstream() -> SocketAddr {
    let sock = UdpSocket::bind("127.0.0.1:0").await.expect("bind upstream");
    let addr = sock.local_addr().expect("local addr");

    tokio::spawn(async move {
        let mut buf = [0u8; 2048];
        loop {
            let Ok((n, src)) = sock.recv_from(&mut buf).await else {
                continue;
            };
            let Ok(req) = Message::from_bytes(&buf[..n]) else {
                continue;
            };
            let Some(q) = req.queries().first() else {
                continue;
            };

            let mut resp = Message::new();
            resp.set_id(req.id());
            resp.set_message_type(MessageType::Response);
            resp.set_op_code(OpCode::Query);
            resp.set_recursion_desired(true);
            resp.set_recursion_available(true);
            resp.set_response_code(ResponseCode::NoError);
            resp.add_queries(req.queries().to_vec());

            if q.query_type() == RecordType::A {
                let record =
                    Record::from_rdata(q.name().clone(), 60, RData::A(A::new(127, 0, 0, 42)));
                resp.add_answer(record);
            }

            let Ok(bytes) = resp.to_bytes() else { continue };
            let _ = sock.send_to(&bytes, src).await;
        }
    });

    addr
}

#[tokio::test]
async fn server_forwards_successfully_to_upstream() {
    // Start a real UDP DNS responder.
    let upstream_addr = start_mock_upstream().await;

    // Build config pointing at the mock upstream.
    let config = DnsConfig {
        enabled: true,
        upstream_servers: vec![wardnet_types::dns::UpstreamDns {
            address: upstream_addr.ip().to_string(),
            name: "mock".to_owned(),
            port: Some(upstream_addr.port()),
            protocol: wardnet_types::dns::DnsProtocol::Udp,
        }],
        cache_size: 100,
        cache_ttl_min_secs: 0,
        cache_ttl_max_secs: 3600,
        ..DnsConfig::default()
    };

    let socket = Arc::new(MockDnsSocket::new());
    let server = UdpDnsServer::with_socket(config, Arc::clone(&socket) as Arc<dyn DnsSocket>);

    server.start().await.unwrap();

    let query_id: u16 = 0x4242;
    let packet = build_dns_query("example.test.", query_id);
    socket.push_packet(packet, client_addr()).await;

    // Poll for response (upstream is local, should be fast).
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if !socket.sent_packets().await.is_empty() {
            break;
        }
    }

    let sent = socket.sent_packets().await;
    assert!(!sent.is_empty(), "server should forward and respond");

    let resp = Message::from_bytes(&sent[0].0).expect("valid DNS response");
    assert_eq!(resp.id(), query_id);
    assert_eq!(resp.response_code(), ResponseCode::NoError);
    assert!(!resp.answers().is_empty(), "response should have answers");

    // Second identical query should hit cache.
    let cache_size_before = server.cache_size().await;
    assert!(cache_size_before >= 1, "first query should have cached");

    let packet2 = build_dns_query("example.test.", 0x4343);
    socket.push_packet(packet2, client_addr()).await;

    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if socket.sent_packets().await.len() >= 2 {
            break;
        }
    }

    let sent2 = socket.sent_packets().await;
    assert!(
        sent2.len() >= 2,
        "second query should get response from cache"
    );

    let cache_hit_rate = server.cache_hit_rate().await;
    assert!(cache_hit_rate > 0.0, "should have at least one cache hit");

    server.stop().await.unwrap();
}

#[tokio::test]
async fn server_query_without_questions_is_ignored() {
    let socket = Arc::new(MockDnsSocket::new());
    let server =
        UdpDnsServer::with_socket(test_config(), Arc::clone(&socket) as Arc<dyn DnsSocket>);

    server.start().await.unwrap();

    // Build a DNS message with no queries (valid wire format, but no question).
    let mut msg = Message::new();
    msg.set_id(0x9999);
    msg.set_message_type(MessageType::Query);
    msg.set_op_code(OpCode::Query);
    let bytes = msg.to_bytes().unwrap();

    socket.push_packet(bytes, client_addr()).await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    // handle_query returns early without sending. No response expected.
    let sent = socket.sent_packets().await;
    assert!(sent.is_empty(), "no response for query without questions");

    assert!(server.is_running());
    server.stop().await.unwrap();
}

// ---------------------------------------------------------------------------
// build_resolver coverage — test all protocol branches.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn build_resolver_with_tls_protocol_falls_back() {
    // TLS protocol triggers the warn-and-fallback branch. Server should still start.
    let config = DnsConfig {
        upstream_servers: vec![wardnet_types::dns::UpstreamDns {
            address: "1.1.1.1".to_owned(),
            name: "Cloudflare TLS".to_owned(),
            port: Some(853),
            protocol: wardnet_types::dns::DnsProtocol::Tls,
        }],
        ..DnsConfig::default()
    };
    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());
    server.start().await.unwrap();
    assert!(server.is_running());
    server.stop().await.unwrap();
}

#[tokio::test]
async fn build_resolver_with_https_protocol_falls_back() {
    // HTTPS protocol triggers the warn-and-fallback branch.
    let config = DnsConfig {
        upstream_servers: vec![wardnet_types::dns::UpstreamDns {
            address: "1.1.1.1".to_owned(),
            name: "Cloudflare HTTPS".to_owned(),
            port: Some(443),
            protocol: wardnet_types::dns::DnsProtocol::Https,
        }],
        ..DnsConfig::default()
    };
    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());
    server.start().await.unwrap();
    assert!(server.is_running());
    server.stop().await.unwrap();
}

#[tokio::test]
async fn build_resolver_with_invalid_ip_skips_upstream() {
    // Non-IP address should be skipped with a warning. Fallback to Cloudflare.
    let config = DnsConfig {
        upstream_servers: vec![wardnet_types::dns::UpstreamDns {
            address: "not-an-ip".to_owned(),
            name: "bad".to_owned(),
            port: Some(53),
            protocol: wardnet_types::dns::DnsProtocol::Udp,
        }],
        ..DnsConfig::default()
    };
    let server = UdpDnsServer::with_bind_addr(config, loopback_ephemeral());
    server.start().await.unwrap();
    assert!(server.is_running());
    server.stop().await.unwrap();
}
