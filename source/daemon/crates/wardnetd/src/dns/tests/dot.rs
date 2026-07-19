//! In-process tests for the `DoT` `:853` listener (issue #912): SNI
//! authentication (token accept, apex/unknown/foreign reject), RFC 7766
//! framing, pipelining with out-of-order completion, the idle timeout, and
//! cert hot-rotation via `reload_from_pem`.
//!
//! The server runs on an ephemeral loopback port with a self-signed cert
//! seeded through the same [`build_serving_control`] path production uses;
//! the test client is rustls with verification disabled (the cert chain is
//! not what's under test — the SNI routing and framing are).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use hickory_proto::op::{Message, ResponseCode};
use hickory_proto::rr::RecordType;
use hickory_proto::serialize::binary::BinDecodable;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;
use wardnet_common::dns::{DnsConfig, DnsRecordType};
use wardnetd_services::CertActivator;
use wardnetd_services::dns::authoritative::AuthoritativeView;
use wardnetd_services::dns::log_sink::DnsLogSink;
use wardnetd_services::error::AppError;
use wardnetd_services::private_dns::{
    DotServer, PrivateDnsGrant, PrivateDnsService, PrivateDnsStatus,
};

use super::pipeline::{build_pipeline, manual_record, query_bytes};
use crate::dns::dot::{DotDnsServer, token_from_sni};
use crate::tls_server::{ServingControl, build_serving_control, install_crypto_provider};

const DOMAIN: &str = "casa.my.wardnet.services";
const TOKEN: &str = "mgrmzsc2ytemzsg4";

// -- Mock PrivateDnsService: one live token --------------------------------

struct SingleTokenResolver {
    token: String,
    device_id: Uuid,
}

#[async_trait]
impl PrivateDnsService for SingleTokenResolver {
    async fn resolve_token(&self, token: &str) -> Result<Option<PrivateDnsGrant>, AppError> {
        Ok((token == self.token).then(|| PrivateDnsGrant {
            id: Uuid::new_v4(),
            device_id: self.device_id,
            token: self.token.clone(),
            created_at: "2026-07-19T00:00:00Z".to_owned(),
        }))
    }
    async fn status(&self) -> Result<PrivateDnsStatus, AppError> {
        Ok(PrivateDnsStatus {
            enabled: true,
            domain: Some(DOMAIN.to_owned()),
        })
    }
    async fn is_enabled(&self) -> Result<bool, AppError> {
        Ok(true)
    }
    async fn set_enabled(&self, _enabled: bool) -> Result<PrivateDnsStatus, AppError> {
        unimplemented!("not exercised by DoT tests")
    }
    async fn grant_device(&self, _device_id: Uuid) -> Result<PrivateDnsGrant, AppError> {
        unimplemented!("not exercised by DoT tests")
    }
    async fn revoke_grant(&self, _grant_id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by DoT tests")
    }
    async fn list_grants(&self) -> Result<Vec<PrivateDnsGrant>, AppError> {
        unimplemented!("not exercised by DoT tests")
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        Ok(())
    }
}

// -- Server / client harness -----------------------------------------------

fn self_signed(domain: &str) -> (Vec<u8>, Vec<u8>) {
    let key_pair = rcgen::KeyPair::generate().expect("keypair");
    let params = rcgen::CertificateParams::new(vec![domain.to_owned(), format!("*.{domain}")])
        .expect("params");
    let cert = params.self_signed(&key_pair).expect("self-signed");
    (
        cert.pem().into_bytes(),
        key_pair.serialize_pem().into_bytes(),
    )
}

struct Harness {
    server: Arc<DotDnsServer>,
    addr: SocketAddr,
    serving: Arc<ServingControl>,
    device_id: Uuid,
    log_rx: tokio::sync::mpsc::Receiver<wardnetd_data::repository::QueryLogRow>,
}

/// Start a `DoT` server on an ephemeral port whose pipeline answers
/// `example.com A` authoritatively (no network) and logs to `log_rx`.
async fn start_server(idle_timeout: Duration) -> Harness {
    start_server_with_rate(idle_timeout, 0).await
}

/// As [`start_server`], but with a non-zero per-client rate limit so the
/// per-token limiting path can be exercised.
async fn start_server_with_rate(idle_timeout: Duration, rate: u32) -> Harness {
    install_crypto_provider();
    let (cert, key) = self_signed(DOMAIN);
    let (rustls_config, serving) =
        build_serving_control(Some((cert, key)), Some(DOMAIN.to_owned()))
            .await
            .expect("serving control");

    let (sink, log_rx) = DnsLogSink::new();
    let cfg = DnsConfig {
        rate_limit_per_second: rate,
        ..DnsConfig::default()
    };
    let pipeline = build_pipeline(cfg, Some(sink));
    pipeline
        .authoritative_view
        .store(Arc::new(AuthoritativeView::build(
            &[],
            vec![manual_record("example.com", DnsRecordType::A, "1.2.3.4")],
            vec![],
        )));

    let device_id = Uuid::new_v4();
    let server = Arc::new(
        DotDnsServer::with_bind_addr(
            "127.0.0.1:0".parse().expect("bind addr"),
            pipeline,
            rustls_config,
            serving.clone(),
            Arc::new(SingleTokenResolver {
                token: TOKEN.to_owned(),
                device_id,
            }),
        )
        .with_timeouts(idle_timeout, Duration::from_secs(5)),
    );
    server.start().await.expect("dot server starts");
    let addr = server.local_addr().expect("bound addr");
    Harness {
        server,
        addr,
        serving,
        device_id,
        log_rx,
    }
}

/// Certificate verification is disabled — the tests pin SNI routing and
/// framing, not chain validation (the production chain is publicly trusted
/// and outside this harness's reach).
#[derive(Debug)]
struct AcceptAnyCert;

impl rustls::client::danger::ServerCertVerifier for AcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Open a TLS connection to `addr` presenting `sni`. No ALPN — exactly what
/// Android's Private DNS client sends.
async fn dot_connect(addr: SocketAddr, sni: &str) -> tokio_rustls::client::TlsStream<TcpStream> {
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAnyCert))
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let tcp = TcpStream::connect(addr).await.expect("tcp connect");
    let name = ServerName::try_from(sni.to_owned()).expect("server name");
    connector.connect(name, tcp).await.expect("tls handshake")
}

async fn send_frame<S: AsyncWriteExt + Unpin>(stream: &mut S, bytes: &[u8]) {
    let len = u16::try_from(bytes.len()).expect("frame fits");
    stream
        .write_all(&len.to_be_bytes())
        .await
        .expect("write length");
    stream.write_all(bytes).await.expect("write frame");
    stream.flush().await.expect("flush");
}

async fn read_frame<S: AsyncReadExt + Unpin>(stream: &mut S) -> Option<Vec<u8>> {
    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf).await.ok()?;
    let mut buf = vec![0u8; usize::from(u16::from_be_bytes(len_buf))];
    stream.read_exact(&mut buf).await.ok()?;
    Some(buf)
}

/// `true` once the peer has closed the stream: a read yields EOF or an
/// error (an abrupt close without `close_notify` surfaces as
/// `UnexpectedEof` in rustls).
async fn connection_is_closed<S: AsyncReadExt + Unpin>(stream: &mut S) -> bool {
    let mut buf = [0u8; 1];
    matches!(
        tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await,
        Ok(Ok(0) | Err(_))
    )
}

fn token_hostname() -> String {
    format!("{TOKEN}.{DOMAIN}")
}

// -- SNI authentication ----------------------------------------------------

#[tokio::test]
async fn token_sni_is_served_and_attributed_to_the_device() {
    let mut h = start_server(Duration::from_secs(30)).await;
    let mut stream = dot_connect(h.addr, &token_hostname()).await;

    send_frame(
        &mut stream,
        &query_bytes(0x0101, "example.com", RecordType::A),
    )
    .await;
    let frame = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut stream))
        .await
        .expect("response within 5s")
        .expect("one response frame");
    let response = Message::from_bytes(&frame).expect("parse response");
    assert_eq!(response.metadata.id, 0x0101);
    assert_eq!(response.answers.len(), 1);
    assert!(response.answers[0].to_string().contains("1.2.3.4"));

    // Attribution: the log row carries the granted device and protocol=dot,
    // even though the source IP (loopback) is in no device snapshot.
    let row = tokio::time::timeout(Duration::from_secs(5), h.log_rx.recv())
        .await
        .expect("log row within 5s")
        .expect("sink open");
    assert_eq!(row.protocol, "dot");
    assert_eq!(
        row.device_id.as_deref(),
        Some(h.device_id.to_string()).as_deref()
    );

    h.server.stop().await.expect("stop");
}

#[tokio::test]
async fn apex_sni_is_refused() {
    let h = start_server(Duration::from_secs(30)).await;
    let mut stream = dot_connect(h.addr, DOMAIN).await;
    assert!(
        connection_is_closed(&mut stream).await,
        "the bare apex must be closed unanswered — closed resolver"
    );
    h.server.stop().await.expect("stop");
}

#[tokio::test]
async fn unknown_token_sni_is_refused() {
    let h = start_server(Duration::from_secs(30)).await;
    let mut stream = dot_connect(h.addr, &format!("wrongtoken234567.{DOMAIN}")).await;
    assert!(
        connection_is_closed(&mut stream).await,
        "an unknown token must be closed unanswered"
    );
    h.server.stop().await.expect("stop");
}

#[tokio::test]
async fn foreign_domain_sni_is_refused() {
    let h = start_server(Duration::from_secs(30)).await;
    let mut stream = dot_connect(h.addr, "sometoken2345678.other.example.net").await;
    assert!(
        connection_is_closed(&mut stream).await,
        "an SNI outside the served domain must be closed unanswered"
    );
    h.server.stop().await.expect("stop");
}

// -- Framing + pipelining --------------------------------------------------

#[tokio::test]
async fn pipelined_queries_are_all_answered() {
    let h = start_server(Duration::from_secs(30)).await;
    let mut stream = dot_connect(h.addr, &token_hostname()).await;

    // Three queries back-to-back without awaiting responses; RFC 7766
    // allows the answers in any order — collect and match by message id.
    let ids = [0x0001u16, 0x0002, 0x0003];
    for id in ids {
        send_frame(&mut stream, &query_bytes(id, "example.com", RecordType::A)).await;
    }

    let mut seen = Vec::new();
    for _ in 0..ids.len() {
        let frame = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut stream))
            .await
            .expect("response within 5s")
            .expect("response frame");
        let response = Message::from_bytes(&frame).expect("parse response");
        assert_eq!(response.answers.len(), 1);
        seen.push(response.metadata.id);
    }
    seen.sort_unstable();
    assert_eq!(
        seen, ids,
        "every pipelined query must be answered exactly once"
    );

    h.server.stop().await.expect("stop");
}

#[tokio::test]
async fn a_query_split_across_writes_is_reassembled() {
    let h = start_server(Duration::from_secs(30)).await;
    let mut stream = dot_connect(h.addr, &token_hostname()).await;

    // Dribble the frame out: length prefix byte-by-byte, then the message
    // in two chunks — the reader must reassemble on the framing boundary,
    // not on write boundaries.
    let query = query_bytes(0x0202, "example.com", RecordType::A);
    let len = u16::try_from(query.len())
        .expect("frame fits")
        .to_be_bytes();
    for chunk in [&len[..1], &len[1..], &query[..5], &query[5..]] {
        stream.write_all(chunk).await.expect("write chunk");
        stream.flush().await.expect("flush");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let frame = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut stream))
        .await
        .expect("response within 5s")
        .expect("response frame");
    let response = Message::from_bytes(&frame).expect("parse response");
    assert_eq!(response.metadata.id, 0x0202);

    h.server.stop().await.expect("stop");
}

// -- Rate limiting ---------------------------------------------------------

#[tokio::test]
async fn rate_limited_query_is_refused_not_disconnected() {
    // Rate 1: the bucket admits a burst of 1, so a rapid second query is
    // shed. It must come back REFUSED with the connection intact (a reset
    // would make a fail-closed client reconnect-storm).
    let h = start_server_with_rate(Duration::from_secs(30), 1).await;
    let mut stream = dot_connect(h.addr, &token_hostname()).await;

    let mut refused = false;
    let mut answered = false;
    for id in 0u16..6 {
        send_frame(&mut stream, &query_bytes(id, "example.com", RecordType::A)).await;
        let frame = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut stream))
            .await
            .expect("a response for every query — connection must stay open")
            .expect("response frame");
        let response = Message::from_bytes(&frame).expect("parse response");
        assert_eq!(response.metadata.id, id);
        match response.metadata.response_code {
            ResponseCode::Refused => refused = true,
            ResponseCode::NoError => answered = true,
            other => panic!("unexpected rcode {other:?}"),
        }
    }
    assert!(answered, "the first (burst) query must be answered");
    assert!(refused, "a later query must be REFUSED, not dropped/reset");

    h.server.stop().await.expect("stop");
}

// -- Idle timeout ----------------------------------------------------------

#[tokio::test]
async fn idle_connection_is_closed() {
    let h = start_server(Duration::from_millis(200)).await;
    let mut stream = dot_connect(h.addr, &token_hostname()).await;
    // Send nothing: the server must drop the connection at the idle bound.
    assert!(
        connection_is_closed(&mut stream).await,
        "an idle connection must be closed by the server"
    );
    h.server.stop().await.expect("stop");
}

// -- Revocation teardown ---------------------------------------------------

#[tokio::test]
async fn revoking_a_device_tears_down_its_live_connection() {
    let h = start_server(Duration::from_secs(30)).await;
    let mut stream = dot_connect(h.addr, &token_hostname()).await;

    // Confirm the session is live and serving.
    send_frame(
        &mut stream,
        &query_bytes(0x0404, "example.com", RecordType::A),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(5), read_frame(&mut stream))
        .await
        .expect("response within 5s")
        .expect("response frame");

    // Revoke the device (the DotRunner would call this on the event); the
    // established connection must be closed, not left resolving.
    h.server.disconnect_device(h.device_id);
    assert!(
        connection_is_closed(&mut stream).await,
        "a revoked device's live connection must be torn down"
    );

    h.server.stop().await.expect("stop");
}

// -- Clean shutdown with a live connection ---------------------------------

#[tokio::test]
async fn stop_completes_promptly_with_a_live_connection() {
    let h = start_server(Duration::from_secs(30)).await;
    let _stream = dot_connect(h.addr, &token_hostname()).await;

    // An open (idle) connection must not wedge stop() — the drain path
    // cancels every connection's token, so this returns well within the
    // bound rather than blocking on the 30 s idle timeout or a stalled
    // writer.
    tokio::time::timeout(Duration::from_secs(5), h.server.stop())
        .await
        .expect("stop must not hang on an open connection")
        .expect("stop");
    assert!(!h.server.is_running());
}

// -- Cert hot-rotation -----------------------------------------------------

#[tokio::test]
async fn cert_rotation_is_picked_up_without_restart() {
    let h = start_server(Duration::from_secs(30)).await;

    let stream = dot_connect(h.addr, &token_hostname()).await;
    let first_cert = stream
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certs| {
            certs
                .first()
                .cloned()
                .map(rustls::pki_types::CertificateDer::into_owned)
        })
        .expect("server presented a cert");
    drop(stream);

    // Rotate the live cert exactly the way TlsRenewalRunner does — through
    // CertActivator::activate → RustlsConfig::reload_from_pem. No DoT
    // restart involved.
    let (new_cert, new_key) = self_signed(DOMAIN);
    h.serving
        .activate(new_cert, new_key, DOMAIN.to_owned())
        .await
        .expect("hot rotation");

    let mut stream = dot_connect(h.addr, &token_hostname()).await;
    let second_cert = stream
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certs| {
            certs
                .first()
                .cloned()
                .map(rustls::pki_types::CertificateDer::into_owned)
        })
        .expect("server presented a cert");
    assert_ne!(
        first_cert, second_cert,
        "the listener must serve the rotated cert on the next connection"
    );

    // And the rotated connection still authenticates + serves.
    send_frame(
        &mut stream,
        &query_bytes(0x0303, "example.com", RecordType::A),
    )
    .await;
    let frame = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut stream))
        .await
        .expect("response within 5s")
        .expect("response frame");
    assert_eq!(
        Message::from_bytes(&frame).expect("parse").metadata.id,
        0x0303
    );

    h.server.stop().await.expect("stop");
}

// -- Lifecycle -------------------------------------------------------------

#[tokio::test]
async fn stop_is_idempotent_and_start_stop_cycles() {
    let h = start_server(Duration::from_secs(30)).await;
    assert!(h.server.is_running());
    h.server.stop().await.expect("stop");
    assert!(!h.server.is_running());
    h.server.stop().await.expect("second stop is a no-op");
    h.server.start().await.expect("restart");
    assert!(h.server.is_running());
    h.server.stop().await.expect("final stop");
}

// -- SNI parsing -----------------------------------------------------------

#[test]
fn token_from_sni_accepts_exactly_one_label_under_the_fqdn() {
    assert_eq!(token_from_sni(&token_hostname(), DOMAIN), Some(TOKEN));
    // Trailing dot is tolerated (DNS names are often written rooted).
    assert_eq!(
        token_from_sni(&format!("{TOKEN}.{DOMAIN}."), DOMAIN),
        Some(TOKEN)
    );
    // The apex, multi-label prefixes, foreign suffixes, and a non-boundary
    // suffix match are all rejected.
    assert_eq!(token_from_sni(DOMAIN, DOMAIN), None);
    assert_eq!(token_from_sni(&format!("a.b.{DOMAIN}"), DOMAIN), None);
    assert_eq!(token_from_sni("token.other.example.net", DOMAIN), None);
    assert_eq!(token_from_sni(&format!("evil{DOMAIN}"), DOMAIN), None);
}
