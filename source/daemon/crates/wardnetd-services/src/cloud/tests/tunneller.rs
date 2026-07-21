//! Tests for the reverse-tunnel client + runner.
//!
//! Pure tests pin the frame codec (byte-for-byte against `wardnet-cloud`'s
//! `handler.rs`, a separate workspace) and the backoff schedule. The integration
//! tests drive the real [`TunnelerRunner`] against a hand-rolled fake Tunneller
//! built on `axum`'s own `ws` support (already a dependency): one exercises the
//! `PING`→`PONG` + `CONNECT`→`DATA`→echo relay to a loopback UDP target, the other
//! that a dropped connection is re-established with backoff.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message as AxMessage, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::{get, post};
use base64::Engine as _;
use serde_json::json;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_data::secret_store::SecretStore;

use crate::cloud::TenantsClient;
use crate::cloud::tunneller::{
    self, FRAME_CLOSE, FRAME_CONNECT, FRAME_DATA, FRAME_PING, FRAME_PONG, FRAME_READY, Frame,
};
use crate::cloud::tunneller_runner::{
    Backoff, Conn, TunnelerConnector, TunnelerRunner, connect_dot_relay, handle_frame,
};
use crate::ddns::region::RegionEndpoint;

// ── Pure: frame codec ───────────────────────────────────────────────────────────

#[test]
fn encode_decode_data_round_trips() {
    let frame = tunneller::encode_data(0x0102_0304, b"hello");
    assert_eq!(frame[0], FRAME_DATA);
    assert_eq!(&frame[1..5], &[0x01, 0x02, 0x03, 0x04]);
    assert_eq!(&frame[5..], b"hello");
    assert_eq!(
        tunneller::decode(&frame),
        Some(Frame::Data {
            conn_id: 0x0102_0304,
            payload: b"hello".to_vec(),
        })
    );
}

#[test]
fn encode_close_and_pong_have_fixed_shape() {
    let close = tunneller::encode_close(7);
    assert_eq!(close, vec![FRAME_CLOSE, 0, 0, 0, 7]);
    assert_eq!(tunneller::decode(&close), Some(Frame::Close { conn_id: 7 }));

    let pong = tunneller::encode_pong();
    assert_eq!(pong, vec![FRAME_PONG, 0, 0, 0, 0]);
}

#[test]
fn encode_ready_has_fixed_shape() {
    // FRAME_READY is [type, conn_id:u32be] — the TCP path's "local connect ok".
    assert_eq!(tunneller::encode_ready(9), vec![FRAME_READY, 0, 0, 0, 9]);
    // It only ever flows daemon→node, so it must not decode as an inbound frame.
    assert_eq!(tunneller::decode(&tunneller::encode_ready(9)), None);
}

#[test]
fn decode_connect_reads_dest_port() {
    // [FRAME_CONNECT, conn_id=9, dest_port=0x0050 (80)]
    let bytes = [FRAME_CONNECT, 0, 0, 0, 9, 0x00, 0x50];
    assert_eq!(
        tunneller::decode(&bytes),
        Some(Frame::Connect {
            conn_id: 9,
            dest_port: 80,
        })
    );
}

#[test]
fn decode_ping_requires_zero_conn_id() {
    assert_eq!(
        tunneller::decode(&[FRAME_PING, 0, 0, 0, 0]),
        Some(Frame::Ping)
    );
    // A ping with a non-zero conn_id is malformed per the protocol → ignored.
    assert_eq!(tunneller::decode(&[FRAME_PING, 0, 0, 0, 1]), None);
}

#[test]
fn decode_rejects_short_unknown_and_unused_frames() {
    // Too short (< 5 bytes).
    assert_eq!(tunneller::decode(&[FRAME_DATA, 0, 0]), None);
    // FRAME_READY (0x02) is daemon→node only, never a valid inbound frame → ignored.
    assert_eq!(tunneller::decode(&[FRAME_READY, 0, 0, 0, 1]), None);
    // A CONNECT truncated before its dest_port.
    assert_eq!(tunneller::decode(&[FRAME_CONNECT, 0, 0, 0, 1]), None);
}

// ── Pure: backoff schedule ──────────────────────────────────────────────────────

#[test]
fn backoff_doubles_caps_and_resets() {
    let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_mins(1));
    let seq: Vec<u64> = (0..8).map(|_| backoff.next().as_secs()).collect();
    assert_eq!(seq, vec![1, 2, 4, 8, 16, 32, 60, 60]);

    backoff.reset();
    assert_eq!(backoff.next().as_secs(), 1);
    assert_eq!(backoff.next().as_secs(), 2);
}

// ── Test doubles ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct MockSystemConfig {
    store: Mutex<HashMap<String, String>>,
}

impl MockSystemConfig {
    fn with(pairs: &[(&str, &str)]) -> Self {
        let store = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        Self {
            store: Mutex::new(store),
        }
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfig {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.store.lock().unwrap().remove(key);
        Ok(())
    }
    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(0)
    }
}

#[derive(Default)]
struct MockSecretStore {
    store: Mutex<HashMap<String, Vec<u8>>>,
}

#[async_trait]
impl SecretStore for MockSecretStore {
    async fn put(&self, path: &str, value: &[u8]) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(path.to_owned(), value.to_vec());
        Ok(())
    }
    async fn get(&self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.store.lock().unwrap().get(path).cloned())
    }
    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        self.store.lock().unwrap().remove(path);
        Ok(())
    }
    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

/// A JWT-shaped string whose payload decodes to `{"exp": <far future>}`.
fn fake_jwt() -> String {
    let exp = chrono::Utc::now().timestamp() + 3_600;
    let payload =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{{\"exp\":{exp}}}"));
    format!("header.{payload}.sig")
}

/// Build a connector pointed at `base_url` (both the token-mint gateway and — via a
/// single-entry catalog — the tunnel gateway), enabled and relaying to `relay_port`.
fn connector(base_url: &str, relay_port: u16) -> Arc<TunnelerConnector> {
    let system_config = Arc::new(MockSystemConfig::with(&[
        ("inbound_wg_enabled", "true"),
        ("inbound_wg_listen_port", &relay_port.to_string()),
        ("ddns_region", "test"),
    ]));
    let secrets = Arc::new(MockSecretStore::default());
    // Seed the enrollment signing key (any 32 bytes — the fake server verifies none).
    {
        let mut store = secrets.store.lock().unwrap();
        store.insert("ddns/daemon/signing_key".to_owned(), vec![7u8; 32]);
    }
    let tenants = Arc::new(TenantsClient::new(
        reqwest::Client::new(),
        base_url.to_owned(),
    ));
    let catalog = vec![RegionEndpoint {
        slug: "test".to_owned(),
        gateway_base_url: base_url.to_owned(),
        health_url: String::new(),
    }];
    Arc::new(TunnelerConnector::new(
        system_config,
        secrets,
        tenants,
        crate::entitlement::Entitlement::shared(),
        catalog,
    ))
}

/// Build a connector whose `inbound_wg_listen_port` starts at `port`, returning it
/// alongside the mock config handle so a test can change the port live. Only the
/// config is exercised here (`handle_frame`'s port re-read); the gateway/secret
/// fields are unused placeholders.
fn connector_with_config(port: u16) -> (Arc<TunnelerConnector>, Arc<MockSystemConfig>) {
    let port_str = port.to_string();
    let system_config = Arc::new(MockSystemConfig::with(&[
        ("inbound_wg_enabled", "true"),
        ("inbound_wg_listen_port", &port_str),
        ("ddns_region", "test"),
    ]));
    let secrets = Arc::new(MockSecretStore::default());
    let tenants = Arc::new(TenantsClient::new(
        reqwest::Client::new(),
        "http://unused".to_owned(),
    ));
    let catalog = vec![RegionEndpoint {
        slug: "test".to_owned(),
        gateway_base_url: "http://unused".to_owned(),
        health_url: String::new(),
    }];
    let connector = Arc::new(TunnelerConnector::new(
        system_config.clone(),
        secrets,
        tenants,
        crate::entitlement::Entitlement::shared(),
        catalog,
    ));
    (connector, system_config)
}

/// Build a connector with the two feature gates set explicitly, so a test can drive
/// the per-frame gating (WG-only / private-DNS-only / both off). The gateway/secret
/// fields are unused placeholders — only the config flags are exercised.
fn connector_with_gates(wg: bool, private_dns: bool) -> Arc<TunnelerConnector> {
    let system_config = Arc::new(MockSystemConfig::with(&[
        ("inbound_wg_enabled", if wg { "true" } else { "false" }),
        (
            "private_dns_enabled",
            if private_dns { "true" } else { "false" },
        ),
        ("inbound_wg_listen_port", "9000"),
        ("ddns_region", "test"),
    ]));
    let secrets = Arc::new(MockSecretStore::default());
    let tenants = Arc::new(TenantsClient::new(
        reqwest::Client::new(),
        "http://unused".to_owned(),
    ));
    let catalog = vec![RegionEndpoint {
        slug: "test".to_owned(),
        gateway_base_url: "http://unused".to_owned(),
        health_url: String::new(),
    }];
    Arc::new(TunnelerConnector::new(
        system_config,
        secrets,
        tenants,
        crate::entitlement::Entitlement::shared(),
        catalog,
    ))
}

/// A relay opener that succeeds with a throwaway loopback socket (no reader task,
/// no real target) — stands in for `open_relay` when a test only cares about the
/// bookkeeping around it.
async fn fake_open_ok(
    _conn_id: u32,
    _port: u16,
    _out_tx: mpsc::Sender<Message>,
) -> std::io::Result<Conn> {
    let socket = UdpSocket::bind(("127.0.0.1", 0)).await?;
    Ok(Conn::new_for_test(Arc::new(socket)))
}

/// A TCP relay opener that succeeds with a throwaway write channel — stands in for
/// `open_tcp_relay` when a test only cares about the bookkeeping around it.
async fn fake_open_tcp_ok(_conn_id: u32, _out_tx: mpsc::Sender<Message>) -> std::io::Result<Conn> {
    let (tx, _rx) = mpsc::channel::<Vec<u8>>(8);
    Ok(Conn::new_tcp_for_test(tx))
}

/// Decode a single binary frame off the out-channel, asserting it is a `FRAME_CLOSE`
/// for `conn_id`.
fn assert_close(msg: Message, conn_id: u32) {
    let Message::Binary(bytes) = msg else {
        panic!("expected a binary frame, got {msg:?}");
    };
    assert_eq!(
        tunneller::decode(bytes.as_ref()),
        Some(Frame::Close { conn_id })
    );
}

// ── Unit: handle_frame flow bookkeeping ───────────────────────────────────────────

#[tokio::test]
async fn open_relay_failure_sends_close() {
    let (connector, _cfg) = connector_with_config(9000);
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);
    let mut conns = HashMap::new();

    // Opener that always fails — the runner must not leave the flow hanging.
    let open = |_conn_id: u32, _port: u16, _out_tx: mpsc::Sender<Message>| async {
        Err::<Conn, _>(std::io::Error::other("forced open_relay failure"))
    };

    handle_frame(
        &connect_frame(1, 51_820),
        &connector,
        9000,
        &out_tx,
        &mut conns,
        &open,
        &fake_open_tcp_ok,
    )
    .await;

    assert!(conns.is_empty(), "a failed open_relay records no conn");
    let msg = out_rx
        .try_recv()
        .expect("open_relay failure must send FRAME_CLOSE");
    assert_close(msg, 1);
}

#[tokio::test]
async fn ninth_concurrent_conn_is_rejected_at_cap() {
    let (connector, _cfg) = connector_with_config(9000);
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);
    let mut conns = HashMap::new();

    // The first MAX_UDP_CONNS (8) flows are accepted with no FRAME_CLOSE.
    for conn_id in 1..=8u32 {
        handle_frame(
            &connect_frame(conn_id, 51_820),
            &connector,
            9000,
            &out_tx,
            &mut conns,
            &fake_open_ok,
            &fake_open_tcp_ok,
        )
        .await;
    }
    assert_eq!(conns.len(), 8, "first 8 conn_ids accepted");
    assert!(
        out_rx.try_recv().is_err(),
        "no FRAME_CLOSE emitted for accepted flows"
    );

    // The 9th is rejected: capped out, FRAME_CLOSE sent, not inserted.
    handle_frame(
        &connect_frame(9, 51_820),
        &connector,
        9000,
        &out_tx,
        &mut conns,
        &fake_open_ok,
        &fake_open_tcp_ok,
    )
    .await;
    assert_eq!(conns.len(), 8, "9th conn_id rejected - cap holds");
    let msg = out_rx
        .try_recv()
        .expect("cap rejection must send FRAME_CLOSE");
    assert_close(msg, 9);
}

#[tokio::test]
async fn connect_rereads_listen_port_per_flow() {
    let (connector, cfg) = connector_with_config(1111);
    let (out_tx, _out_rx) = mpsc::channel::<Message>(256);
    let mut conns = HashMap::new();

    // Record the port each new flow was opened against.
    let recorded: Arc<Mutex<Vec<u16>>> = Arc::new(Mutex::new(Vec::new()));
    let rec = recorded.clone();
    let open = move |_conn_id: u32, port: u16, _out_tx: mpsc::Sender<Message>| {
        let rec = rec.clone();
        async move {
            rec.lock().unwrap().push(port);
            let socket = UdpSocket::bind(("127.0.0.1", 0)).await?;
            Ok(Conn::new_for_test(Arc::new(socket)))
        }
    };

    // First flow relays to the initial configured port.
    handle_frame(
        &connect_frame(1, 0),
        &connector,
        1111,
        &out_tx,
        &mut conns,
        &open,
        &fake_open_tcp_ok,
    )
    .await;
    // Admin changes the listen port live between flows.
    cfg.set("inbound_wg_listen_port", "2222").await.unwrap();
    // The next NEW flow picks up the changed port without a reconnect.
    handle_frame(
        &connect_frame(2, 0),
        &connector,
        1111,
        &out_tx,
        &mut conns,
        &open,
        &fake_open_tcp_ok,
    )
    .await;

    assert_eq!(
        *recorded.lock().unwrap(),
        vec![1111, 2222],
        "each new flow re-reads the live inbound_wg_listen_port"
    );
}

// ── Unit: per-frame gating (issue #913) ───────────────────────────────────────────

/// Drive a single CONNECT through `handle_frame` and report whether it was accepted
/// (recorded in `conns`) or rejected with a `FRAME_CLOSE` — using no-op fake openers,
/// so only the gate + dispatch decision is exercised.
async fn connect_outcome(connector: &Arc<TunnelerConnector>, dest_port: u16) -> (usize, bool) {
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);
    let mut conns = HashMap::new();
    handle_frame(
        &connect_frame(1, dest_port),
        connector,
        9000,
        &out_tx,
        &mut conns,
        &fake_open_ok,
        &fake_open_tcp_ok,
    )
    .await;
    let closed = out_rx.try_recv().is_ok();
    (conns.len(), closed)
}

#[tokio::test]
async fn wg_only_gate_relays_wireguard_and_rejects_dot() {
    let connector = connector_with_gates(true, false);

    // A WireGuard flow (advisory dest_port) is accepted.
    let (accepted, closed) = connect_outcome(&connector, 51_820).await;
    assert_eq!(
        (accepted, closed),
        (1, false),
        "WG flow accepted when WG on"
    );

    // A DoT flow is rejected with FRAME_CLOSE while Private DNS is off.
    let (accepted, closed) = connect_outcome(&connector, 853).await;
    assert_eq!(
        (accepted, closed),
        (0, true),
        "DoT flow closed when DNS off"
    );
}

#[tokio::test]
async fn private_dns_only_gate_relays_dot_and_rejects_wireguard() {
    let connector = connector_with_gates(false, true);

    // A DoT flow is accepted.
    let (accepted, closed) = connect_outcome(&connector, 853).await;
    assert_eq!(
        (accepted, closed),
        (1, false),
        "DoT flow accepted when DNS on"
    );

    // A WireGuard flow is rejected with FRAME_CLOSE while inbound-WG is off.
    let (accepted, closed) = connect_outcome(&connector, 51_820).await;
    assert_eq!((accepted, closed), (0, true), "WG flow closed when WG off");
}

#[tokio::test]
async fn both_gates_off_rejects_every_flow() {
    // With neither feature enabled the outer loop would not even dial, but should a
    // frame still arrive mid-teardown, every flow is closed regardless of transport.
    let connector = connector_with_gates(false, false);

    let (accepted, closed) = connect_outcome(&connector, 51_820).await;
    assert_eq!(
        (accepted, closed),
        (0, true),
        "WG flow closed when both off"
    );
    let (accepted, closed) = connect_outcome(&connector, 853).await;
    assert_eq!(
        (accepted, closed),
        (0, true),
        "DoT flow closed when both off"
    );
}

#[tokio::test]
async fn https_connect_is_deferred_and_closed() {
    // dest_port 443 has no local target yet (#816): always closed, regardless of
    // which feature is enabled.
    let connector = connector_with_gates(true, true);
    let (accepted, closed) = connect_outcome(&connector, 443).await;
    assert_eq!(
        (accepted, closed),
        (0, true),
        "HTTPS (443) closed - deferred"
    );
}

// ── Unit: split caps are independent (issue #913) ─────────────────────────────────

#[tokio::test]
async fn udp_and_tcp_caps_are_enforced_independently() {
    let connector = connector_with_gates(true, true);
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);
    let mut conns = HashMap::new();

    // Fill the TCP (DoT) budget to MAX_TCP_CONNS = 16.
    for conn_id in 1..=16u32 {
        handle_frame(
            &connect_frame(conn_id, 853),
            &connector,
            9000,
            &out_tx,
            &mut conns,
            &fake_open_ok,
            &fake_open_tcp_ok,
        )
        .await;
    }
    assert_eq!(conns.len(), 16, "16 DoT flows accepted");
    assert!(
        out_rx.try_recv().is_err(),
        "no FRAME_CLOSE for accepted DoT"
    );

    // The 17th DoT flow is rejected at the cap.
    handle_frame(
        &connect_frame(17, 853),
        &connector,
        9000,
        &out_tx,
        &mut conns,
        &fake_open_ok,
        &fake_open_tcp_ok,
    )
    .await;
    assert_eq!(conns.len(), 16, "17th DoT flow rejected - TCP cap holds");
    assert_close(out_rx.try_recv().expect("cap rejection sends CLOSE"), 17);

    // A WireGuard flow still opens: the TCP budget being full must not starve UDP.
    handle_frame(
        &connect_frame(100, 51_820),
        &connector,
        9000,
        &out_tx,
        &mut conns,
        &fake_open_ok,
        &fake_open_tcp_ok,
    )
    .await;
    assert_eq!(
        conns.len(),
        17,
        "a UDP flow opens despite a full TCP budget"
    );
    assert!(
        out_rx.try_recv().is_err(),
        "no FRAME_CLOSE for the accepted UDP flow"
    );
}

// ── Integration: DoT TCP relay (issue #913) ───────────────────────────────────────

/// Spawn a loopback TCP server that, per connection, echoes the first chunk it reads
/// and then closes — standing in for the daemon's local `:853` `DoT` listener while
/// also exercising the local-EOF teardown path. Returns its port.
async fn spawn_tcp_echo_then_close() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 2048];
                if let Ok(n @ 1..) = socket.read(&mut buf).await {
                    let _ = socket.write_all(&buf[..n]).await;
                }
                // Drop `socket` → the client sees EOF.
            });
        }
    });
    port
}

/// Bind then immediately drop a listener to obtain a port guaranteed to refuse
/// connections — for the `DoT` connect-failure path.
async fn closed_tcp_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
    // `listener` drops here.
}

#[tokio::test]
async fn dot_relay_sends_ready_first_then_relays_and_closes_on_eof() {
    let echo_port = spawn_tcp_echo_then_close().await;
    let connector = connector_with_gates(false, true);
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);
    let mut conns = HashMap::new();

    // Real DoT opener bound to the loopback echo instead of :853.
    let open_tcp = move |conn_id, out_tx| connect_dot_relay(conn_id, echo_port, out_tx);

    // CONNECT dest_port=853 opens the TCP relay.
    handle_frame(
        &connect_frame(1, 853),
        &connector,
        9000,
        &out_tx,
        &mut conns,
        &fake_open_ok,
        &open_tcp,
    )
    .await;
    assert_eq!(conns.len(), 1, "DoT flow recorded");

    // The very first frame the node sees must be FRAME_READY (before any data).
    let first = timeout(Duration::from_secs(5), out_rx.recv())
        .await
        .expect("READY before timeout")
        .expect("channel open");
    let Message::Binary(bytes) = first else {
        panic!("expected READY frame");
    };
    assert_eq!(
        bytes.as_ref(),
        &[FRAME_READY, 0, 0, 0, 1],
        "READY precedes data"
    );

    // Feed one payload; the echo comes back as FRAME_DATA (bidirectional relay).
    handle_frame(
        &tunneller::encode_data(1, b"ping"),
        &connector,
        9000,
        &out_tx,
        &mut conns,
        &fake_open_ok,
        &open_tcp,
    )
    .await;
    let echoed = timeout(Duration::from_secs(5), out_rx.recv())
        .await
        .expect("echo before timeout")
        .expect("channel open");
    assert_eq!(
        tunneller::decode(match &echoed {
            Message::Binary(b) => b.as_ref(),
            other => panic!("expected binary, got {other:?}"),
        }),
        Some(Frame::Data {
            conn_id: 1,
            payload: b"ping".to_vec(),
        }),
        "the local echo is relayed back as FRAME_DATA"
    );

    // The echo server then closed: the relay reports local EOF as FRAME_CLOSE.
    let closed = timeout(Duration::from_secs(5), out_rx.recv())
        .await
        .expect("CLOSE before timeout")
        .expect("channel open");
    assert_close(closed, 1);
}

#[tokio::test]
async fn dot_connect_failure_sends_close() {
    let dead_port = closed_tcp_port().await;
    let connector = connector_with_gates(false, true);
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);
    let mut conns = HashMap::new();

    let open_tcp = move |conn_id, out_tx| connect_dot_relay(conn_id, dead_port, out_tx);
    handle_frame(
        &connect_frame(1, 853),
        &connector,
        9000,
        &out_tx,
        &mut conns,
        &fake_open_ok,
        &open_tcp,
    )
    .await;

    assert!(conns.is_empty(), "a refused DoT connect records no conn");
    let msg = timeout(Duration::from_secs(5), out_rx.recv())
        .await
        .expect("CLOSE before timeout")
        .expect("channel open");
    assert_close(msg, 1);
}

// ── Fake Tunneller server ───────────────────────────────────────────────────────

struct ServerState {
    connections: AtomicU32,
    /// `(saw_pong, saw_echo)` from the first connection's protocol exchange.
    result_tx: mpsc::Sender<(bool, bool)>,
    /// Fires once a *second* upgrade lands (proves the client reconnected).
    reconnect_tx: mpsc::Sender<()>,
}

async fn token_handler() -> axum::Json<serde_json::Value> {
    axum::Json(json!({ "token": fake_jwt() }))
}

async fn tunnel_handler(ws: WebSocketUpgrade, State(state): State<Arc<ServerState>>) -> Response {
    ws.on_upgrade(move |socket| drive_fake_node(socket, state))
}

/// The fake node: on the first connection it runs the full protocol probe; on any
/// later connection it just signals the reconnect and closes.
async fn drive_fake_node(mut socket: WebSocket, state: Arc<ServerState>) {
    let attempt = state.connections.fetch_add(1, Ordering::SeqCst) + 1;
    if attempt > 1 {
        let _ = state.reconnect_tx.send(()).await;
        let _ = socket.send(AxMessage::Close(None)).await;
        return;
    }

    // App-level ping (conn_id must be 0), then open a flow and send one datagram.
    let _ = socket
        .send(AxMessage::Binary(vec![FRAME_PING, 0, 0, 0, 0].into()))
        .await;
    let _ = socket
        .send(AxMessage::Binary(connect_frame(1, 51_820).into()))
        .await;
    let _ = socket
        .send(AxMessage::Binary(
            tunneller::encode_data(1, b"hello").into(),
        ))
        .await;

    let mut saw_pong = false;
    let mut saw_echo = false;
    while !(saw_pong && saw_echo) {
        match timeout(Duration::from_secs(5), socket.recv()).await {
            Ok(Some(Ok(AxMessage::Binary(bytes)))) => match bytes.first().copied() {
                Some(FRAME_PONG) => saw_pong = true,
                Some(FRAME_DATA) if bytes.len() > 5 && &bytes[5..] == b"hello" => saw_echo = true,
                _ => {}
            },
            _ => break,
        }
    }

    let _ = state.result_tx.send((saw_pong, saw_echo)).await;
    // Close this flow and the socket.
    let _ = socket
        .send(AxMessage::Binary(tunneller::encode_close(1).into()))
        .await;
    let _ = socket.send(AxMessage::Close(None)).await;
}

fn connect_frame(conn_id: u32, dest_port: u16) -> Vec<u8> {
    let mut f = vec![FRAME_CONNECT];
    f.extend_from_slice(&conn_id.to_be_bytes());
    f.extend_from_slice(&dest_port.to_be_bytes());
    f
}

/// Spawn the fake Tunneller + token endpoint on a loopback port; returns its base
/// URL and the two receivers.
async fn spawn_fake_server() -> (String, mpsc::Receiver<(bool, bool)>, mpsc::Receiver<()>) {
    let (result_tx, result_rx) = mpsc::channel(1);
    let (reconnect_tx, reconnect_rx) = mpsc::channel(1);
    let state = Arc::new(ServerState {
        connections: AtomicU32::new(0),
        result_tx,
        reconnect_tx,
    });
    let app = Router::new()
        .route("/tunneller/v1/tunnel", get(tunnel_handler))
        .route("/v1/token", post(token_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), result_rx, reconnect_rx)
}

/// Bind a loopback UDP echo target; returns its port. Every datagram is echoed back
/// to its sender — standing in for the daemon's inbound-WG listener.
async fn spawn_udp_echo() -> u16 {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = socket.local_addr().unwrap().port();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 2048];
        while let Ok((n, from)) = socket.recv_from(&mut buf).await {
            let _ = socket.send_to(&buf[..n], from).await;
        }
    });
    port
}

// ── Integration ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn relays_ping_and_connect_data_to_local_target() {
    let relay_port = spawn_udp_echo().await;
    let (base_url, mut result_rx, _reconnect_rx) = spawn_fake_server().await;

    let runner = TunnelerRunner::start(connector(&base_url, relay_port), &tracing::Span::none());

    let (saw_pong, saw_echo) = timeout(Duration::from_secs(10), result_rx.recv())
        .await
        .expect("fake node reported a result before timeout")
        .expect("result channel open");
    assert!(saw_pong, "daemon must answer FRAME_PING with FRAME_PONG");
    assert!(
        saw_echo,
        "daemon must relay FRAME_DATA to the local target and return the echo"
    );

    runner.shutdown().await;
}

#[tokio::test]
async fn reconnects_after_the_server_drops_the_connection() {
    let relay_port = spawn_udp_echo().await;
    let (base_url, _result_rx, mut reconnect_rx) = spawn_fake_server().await;

    let runner = TunnelerRunner::start(connector(&base_url, relay_port), &tracing::Span::none());

    // First connection runs the probe then closes; the client must dial again
    // (backoff floor is 1s), producing a second upgrade.
    timeout(Duration::from_secs(10), reconnect_rx.recv())
        .await
        .expect("client reconnected before timeout")
        .expect("reconnect channel open");

    runner.shutdown().await;
}
