//! Client for the **tunneller** service — the regional reverse-tunnel relay that
//! carries inbound `WireGuard` UDP down to the daemon over a persistent WebSocket
//! (cloud ADR-0015, `wardnet` ADR-0022, issue #809).
//!
//! Unlike [`DdnsClient`](super::ddns::DdnsClient) — a set of one-shot HTTP calls —
//! this client owns a **long-lived connection**: it dials the region's gateway,
//! upgrades to a WebSocket, and hands the live stream to
//! [`tunneller_runner`](super::tunneller_runner), which relays frames. The
//! reconnect/backoff lifecycle lives in the runner; this module owns only the
//! *dial* (auth + upgrade) and the *wire framing*.
//!
//! ## Auth (identical shape to every other cloud call)
//!
//! The gateway authenticates the same network-scoped JWT + Ed25519 proof-of-
//! possession every [`DdnsClient`](super::ddns::DdnsClient) call carries (cloud
//! ADR-0013). [`request::send`](super::request) is HTTP-only, so this module can't
//! reuse it for the upgrade; instead it computes the **same** `PoP` signature via
//! [`pop::sign`] and attaches the **same** `Authorization` / `X-Wardnet-Timestamp`
//! / `X-Wardnet-Signature` headers to a hand-built WebSocket upgrade request. The
//! signed path is the **full, prefixed** `/tunneller/v1/tunnel` — the gateway is
//! path-preserving and verifies the signature against the un-stripped URI, so the
//! daemon must sign exactly what it dials.
//!
//! ## Frame protocol
//!
//! Byte-identical to `wardnet-cloud`'s `crates/tunneller/src/tunnel/handler.rs`.
//! For the UDP relay path only these matter (`FRAME_READY` 0x02 is the TCP/SNI
//! path's "local connect succeeded" signal and is **never used** for UDP —
//! datagrams flow immediately after `FRAME_CONNECT`):
//!
//! ```text
//! FRAME_CONNECT 0x01  node→daemon  [type, conn_id:u32be, dest_port:u16be]
//! FRAME_DATA    0x03  both         [type, conn_id:u32be, payload...]
//! FRAME_CLOSE   0x04  both         [type, conn_id:u32be]
//! FRAME_PING    0x05  node→daemon  [type, 0u32]   (application-level, not WS ping)
//! FRAME_PONG    0x06  daemon→node  [type, 0u32]
//! ```

use chrono::Utc;
use http::header::AUTHORIZATION;
use http::{HeaderName, HeaderValue};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

use super::CloudError;
use super::identity::DaemonIdentity;
use super::pop;

/// The gateway-facing WebSocket path — **prefixed** with `/tunneller/`, which is
/// load-bearing for the `PoP` signature (the gateway verifies the un-stripped
/// URI, cloud ADR-0015).
pub(crate) const TUNNEL_PATH: &str = "/tunneller/v1/tunnel";

// ── Frame protocol constants (mirror wardnet-cloud `handler.rs`) ────────────────

/// `node→daemon`: a new inbound UDP flow — open a local relay socket for `conn_id`.
pub(crate) const FRAME_CONNECT: u8 = 0x01;
/// Both directions: one relayed datagram for `conn_id`.
pub(crate) const FRAME_DATA: u8 = 0x03;
/// Both directions: tear down `conn_id`.
pub(crate) const FRAME_CLOSE: u8 = 0x04;
/// `node→daemon`: application-level keepalive (`conn_id` must be 0), distinct from
/// the WS-level ping the transport auto-pongs.
pub(crate) const FRAME_PING: u8 = 0x05;
/// `daemon→node`: reply to [`FRAME_PING`] (`conn_id` 0).
pub(crate) const FRAME_PONG: u8 = 0x06;

/// A decoded inbound frame (node→daemon). Anything malformed or unrecognised
/// (including the unused `FRAME_READY`) decodes to [`None`] and is ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Frame {
    /// A new inbound flow. `dest_port` is **advisory only** — the runner relays to
    /// the daemon's own configured `inbound_wg_listen_port`, never a port the
    /// daemon didn't set itself (see the runner's `relay_target`).
    Connect { conn_id: u32, dest_port: u16 },
    /// One relayed datagram to write to `conn_id`'s local socket.
    Data { conn_id: u32, payload: Vec<u8> },
    /// Tear down `conn_id`.
    Close { conn_id: u32 },
    /// Application-level ping (answer with [`encode_pong`]).
    Ping,
}

/// Decode an inbound binary frame, or [`None`] if it is too short, carries an
/// unrecognised type, or is a well-formed frame the daemon does not act on
/// (`FRAME_READY`, or a `FRAME_PING` with a non-zero `conn_id`).
pub(crate) fn decode(data: &[u8]) -> Option<Frame> {
    // Every frame is at least `[type, conn_id:u32be]` = 5 bytes.
    if data.len() < 5 {
        return None;
    }
    let conn_id = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
    match data[0] {
        FRAME_CONNECT if data.len() >= 7 => {
            let dest_port = u16::from_be_bytes([data[5], data[6]]);
            Some(Frame::Connect { conn_id, dest_port })
        }
        FRAME_DATA => Some(Frame::Data {
            conn_id,
            payload: data[5..].to_vec(),
        }),
        FRAME_CLOSE => Some(Frame::Close { conn_id }),
        // The protocol pins `conn_id == 0` for pings; ignore anything else.
        FRAME_PING if conn_id == 0 => Some(Frame::Ping),
        _ => None,
    }
}

/// Encode a `FRAME_DATA` carrying `payload` for `conn_id` (daemon→node).
pub(crate) fn encode_data(conn_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(5 + payload.len());
    f.push(FRAME_DATA);
    f.extend_from_slice(&conn_id.to_be_bytes());
    f.extend_from_slice(payload);
    f
}

/// Encode a `FRAME_CLOSE` for `conn_id` (daemon→node).
pub(crate) fn encode_close(conn_id: u32) -> Vec<u8> {
    let mut f = Vec::with_capacity(5);
    f.push(FRAME_CLOSE);
    f.extend_from_slice(&conn_id.to_be_bytes());
    f
}

/// Encode a `FRAME_PONG` (`conn_id` 0) answering an application-level ping.
pub(crate) fn encode_pong() -> Vec<u8> {
    let mut f = Vec::with_capacity(5);
    f.push(FRAME_PONG);
    f.extend_from_slice(&0u32.to_be_bytes());
    f
}

/// The live tunnel stream handed back by [`TunnelerClient::connect`] — a
/// WebSocket over either plain TCP (tests) or a rustls TLS session (production
/// `wss://`).
pub(crate) type TunnelStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A client for a region's tunneller service behind the regional gateway at
/// `base_url` (the same base the region's [`DdnsClient`](super::ddns::DdnsClient)
/// uses, with `https://`→`wss://`).
pub struct TunnelerClient {
    base_url: String,
}

impl TunnelerClient {
    /// Build a client pointed at the region's gateway `base_url` (e.g.
    /// `https://api.euc.wardnet.network`). The scheme is rewritten to `ws(s)://`
    /// and [`TUNNEL_PATH`] appended at dial time.
    #[must_use]
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }

    /// Dial the gateway and upgrade to the tunnel WebSocket, authenticating with
    /// the network-scoped JWT + `PoP` signature `identity` carries.
    ///
    /// Propagates [`CloudError::EntitlementLost`] when the token mint is refused
    /// (lapsed subscription); any transport / upgrade failure is
    /// [`CloudError::Upstream`].
    pub(crate) async fn connect(
        &self,
        identity: &DaemonIdentity,
    ) -> Result<TunnelStream, CloudError> {
        // Mint (or reuse) the JWT first — this is the call that can surface a
        // lapsed subscription as `EntitlementLost`.
        let token = identity.token().await?;
        let timestamp = Utc::now().timestamp();
        // Sign exactly the path we dial (empty body — a GET upgrade), the same
        // canonical payload the gateway reconstructs.
        let signature = pop::sign(identity.signing_key(), "GET", TUNNEL_PATH, timestamp, b"");

        let url = self.ws_url();
        let mut request = url
            .into_client_request()
            .map_err(|e| CloudError::Upstream(anyhow::anyhow!("invalid tunnel URL: {e}")))?;
        let headers = request.headers_mut();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|e| CloudError::Upstream(e.into()))?,
        );
        headers.insert(
            header_name(pop::TIMESTAMP_HEADER),
            HeaderValue::from_str(&timestamp.to_string())
                .map_err(|e| CloudError::Upstream(e.into()))?,
        );
        headers.insert(
            header_name(pop::SIGNATURE_HEADER),
            HeaderValue::from_str(&signature).map_err(|e| CloudError::Upstream(e.into()))?,
        );

        let (stream, _response) = connect_async(request)
            .await
            .map_err(|e| CloudError::Upstream(anyhow::anyhow!("tunnel WS connect failed: {e}")))?;
        Ok(stream)
    }

    /// Rewrite the HTTP(S) gateway base into a `ws(s)://…/tunneller/v1/tunnel`
    /// URL: `https`→`wss`, `http`→`ws`, other schemes passed through unchanged.
    fn ws_url(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        let ws_base = if let Some(rest) = base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            base.to_owned()
        };
        format!("{ws_base}{TUNNEL_PATH}")
    }
}

/// Parse one of the fixed, always-valid wardnet header-name constants into a
/// [`HeaderName`]. The constants are compile-time literals, so a parse failure is
/// a programming error, not a runtime condition.
fn header_name(name: &str) -> HeaderName {
    HeaderName::from_bytes(name.as_bytes()).expect("wardnet header name is always valid")
}
