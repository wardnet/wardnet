//! Background runner owning the daemon's **one persistent connection**: the
//! reverse tunnel to wardnet-cloud's Tunneller (cloud ADR-0015, `wardnet`
//! ADR-0022, issue #809).
//!
//! This is genuinely new architecture for the daemon — every other background
//! component (`ddns::runner`, `tls::runner`, …) is a fixed-interval poll loop with
//! no long-lived socket. Here the runner:
//!
//! 1. **Gates** on `inbound_wg_enabled() || private_dns_enabled()` — the tunnel
//!    carries two independent features (inbound `WireGuard`, issue #809, and
//!    Private DNS's `:853` `DoT` reachability, issue #913), so it dials while
//!    *either* is on and tears everything down only once *both* are off. Each
//!    inbound flow is additionally gated per-frame on the feature it belongs to,
//!    so turning one feature off rejects its flows without disturbing the other's.
//! 2. **Dials** via [`TunnelerClient`], reusing the same enrollment identity + region
//!    the DDNS client uses (seed + `ddns_region` slug the wizard persisted).
//! 3. **Relays** each `conn_id`, choosing the path by the `FRAME_CONNECT`
//!    `dest_port`:
//!    * `853` (Private DNS): TCP-connect the daemon's own loopback `DoT` listener
//!      (`127.0.0.1:853`, 5s timeout), emit `FRAME_READY` once connected, then
//!      relay bytes both ways until `FRAME_CLOSE`, local EOF, or the idle timeout.
//!    * `443` (HTTPS SNI passthrough): reserved for the reverse web proxy —
//!      deferred to #816, closed immediately for now.
//!    * anything else (inbound `WireGuard`): open a loopback UDP socket to the
//!      daemon's *own* `inbound_wg_listen_port`, deliberately **ignoring** the
//!      frame's advisory `dest_port` (trusting a port the daemon didn't configure
//!      would be a local-relay-target-confusion risk), and forward datagrams.
//! 4. **Reconnects** with exponential backoff (1s → ×2 → 60s cap), reset after a
//!    connection stays up past [`STABLE_CONNECTION`].
//!
//! The relay-socket count is capped per transport ([`MAX_UDP_CONNS`] /
//! [`MAX_TCP_CONNS`]) so churn on one feature cannot starve the other of slots.
//!
//! WS keepalive: the cloud node pings every 30s and closes after 90s idle. The read
//! loop stays hot (so the transport's auto-pong fires) and additionally answers a
//! WS `Ping` with an explicit `Pong`, plus the **application-level** `FRAME_PING`
//! with `FRAME_PONG`.

use std::collections::HashMap;
use std::future::Future;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt as _, StreamExt as _};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use wardnet_common::auth::AuthContext;

use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_data::secret_store::SecretStore;

use super::identity::DaemonIdentity;
use super::tenants::TenantsClient;
use super::tunneller::{self, Frame, TunnelStream, TunnelerClient};
use crate::auth_context;
use crate::ddns::region::RegionEndpoint;
use crate::ddns::{KEY_REGION, SECRET_DAEMON_KEY};
use crate::entitlement::Entitlement;

/// Initial reconnect delay after a dropped/failed connection.
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
/// Ceiling for the exponential reconnect backoff.
const MAX_BACKOFF: Duration = Duration::from_mins(1);
/// A connection that stays up at least this long is treated as healthy, resetting
/// the backoff so a later drop retries fast rather than inheriting a grown delay.
const STABLE_CONNECTION: Duration = Duration::from_secs(30);
/// How often the outer loop re-checks the gate while disabled / not-yet-enrolled.
const GATE_POLL_INTERVAL: Duration = Duration::from_secs(5);
/// How often, *while connected*, the loop re-checks the `inbound_wg_enabled` gate so
/// a disable takes effect without waiting for the next inbound frame.
const DISABLE_CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// How often the loop sweeps idle `conn_id`s.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(5);
/// Idle window after which a relayed UDP `conn_id` is torn down and `FRAME_CLOSE`
/// sent proactively. Mirrors `wardnet-cloud`'s `UDP_IDLE_TIMEOUT`: UDP/`WireGuard`
/// has no explicit teardown, so this NAT-style timeout (2 min) comfortably outlasts
/// several missed keepalives (default 25s) before reclaiming a live-but-quiet peer.
const UDP_IDLE_TIMEOUT: Duration = Duration::from_mins(2);

/// Idle window for a relayed TCP (`DoT`) `conn_id`. TCP tears down explicitly on
/// EOF/`FRAME_CLOSE`, so this is only a backstop against a half-open connection that
/// stops moving bytes without ever closing; a `DoT` query/response is short-lived,
/// so 2 min is generous headroom.
const TCP_IDLE_TIMEOUT: Duration = Duration::from_mins(2);

/// The daemon's own loopback `DoT` listener port. A Private-DNS `FRAME_CONNECT`
/// (`dest_port == 853`) is relayed here — never to a port the frame carries.
const DOT_PORT: u16 = 853;
/// HTTPS SNI-passthrough port. The cloud edge can emit this once the reverse web
/// proxy exists; until #816 the daemon has no local `:443` target and closes it.
const HTTPS_PORT: u16 = 443;
/// How long to wait for the loopback `DoT` connect before giving up and closing the
/// flow, so a stuck local listener can't wedge a `conn_id`.
const TCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Upper bound on concurrently relayed **UDP** `conn_id`s over one WS connection.
/// Mirrors `wardnet-cloud`'s own `UDP_MAX_CONNS = 8` as **defense-in-depth**: the
/// cloud already caps its side, and the daemon independently caps its own so a
/// misbehaving or compromised node cannot make it open an unbounded number of local
/// relay sockets before the idle sweep reclaims them.
const MAX_UDP_CONNS: usize = 8;
/// Upper bound on concurrently relayed **TCP** (`DoT`) `conn_id`s. Kept separate
/// from [`MAX_UDP_CONNS`] so a burst of Private-DNS connections cannot exhaust the
/// slots inbound `WireGuard` needs (and vice versa) — each transport gets its own
/// independent budget.
const MAX_TCP_CONNS: usize = 16;

/// Exponential backoff schedule: hands out `initial`, then doubles each call up to
/// `max`; [`reset`](Self::reset) returns to `initial`. Pure and deterministic so the
/// schedule is unit-testable without any timing.
pub(crate) struct Backoff {
    current: Duration,
    initial: Duration,
    max: Duration,
}

impl Backoff {
    pub(crate) fn new(initial: Duration, max: Duration) -> Self {
        Self {
            current: initial,
            initial,
            max,
        }
    }

    /// The current delay, advancing the schedule (doubling, capped at `max`).
    pub(crate) fn next(&mut self) -> Duration {
        let delay = self.current;
        self.current = (self.current * 2).min(self.max);
        delay
    }

    /// Return to the initial delay (after a healthy connection).
    pub(crate) fn reset(&mut self) {
        self.current = self.initial;
    }
}

/// Everything the runner needs to (re)establish the reverse tunnel: the config +
/// secret access to read the gate/port and enrollment identity, the region catalog
/// to resolve the gateway, and the shared entitlement the token mints flip.
///
/// Built once at service-init and shared with the runner; the daemon has exactly
/// one, mirroring how the DDNS runner is handed one `Arc<dyn DdnsService>`.
pub struct TunnelerConnector {
    system_config: Arc<dyn SystemConfigRepository>,
    secrets: Arc<dyn SecretStore>,
    tenants: Arc<TenantsClient>,
    entitlement: Arc<Entitlement>,
    region_catalog: Vec<RegionEndpoint>,
}

impl TunnelerConnector {
    /// Assemble the connector from the shared config/secret handles, a `tenants`
    /// client bound to the global gateway (for token minting), the entitlement
    /// handle, and the region catalog (resolves the slug the enrollment persisted
    /// to the regional gateway base URL).
    #[must_use]
    pub fn new(
        system_config: Arc<dyn SystemConfigRepository>,
        secrets: Arc<dyn SecretStore>,
        tenants: Arc<TenantsClient>,
        entitlement: Arc<Entitlement>,
        region_catalog: Vec<RegionEndpoint>,
    ) -> Self {
        Self {
            system_config,
            secrets,
            tenants,
            entitlement,
            region_catalog,
        }
    }

    /// The runner's outer gate: the tunnel is wanted while *either* feature it
    /// carries is enabled. A read error on one flag is treated as that feature
    /// being off (already logged by the per-feature reader).
    async fn enabled(&self) -> bool {
        self.wg_enabled().await || self.private_dns_enabled().await
    }

    /// Whether the inbound `WireGuard` server (issue #809) is enabled — the gate
    /// for the UDP relay path.
    async fn wg_enabled(&self) -> bool {
        match self.system_config.inbound_wg_enabled().await {
            Ok(enabled) => enabled,
            Err(error) => {
                tracing::warn!(%error, "reverse tunnel: failed to read inbound-wg enabled flag");
                false
            }
        }
    }

    /// Whether Private DNS (issue #913) is enabled — the gate for the `:853` TCP
    /// relay path.
    async fn private_dns_enabled(&self) -> bool {
        match self.system_config.private_dns_enabled().await {
            Ok(enabled) => enabled,
            Err(error) => {
                tracing::warn!(%error, "reverse tunnel: failed to read private-dns enabled flag");
                false
            }
        }
    }

    /// Resolve the identity, regional gateway base URL, and local relay port for a
    /// connection attempt. [`None`] when the box has not completed DDNS enrollment
    /// (no persisted region slug or signing seed) — the runner then idles and
    /// retries, exactly as it does while disabled.
    ///
    /// The reads target `system_config`/`SecretStore` directly (the same
    /// `ddns_region` slug + signing seed the DDNS client uses) rather than routing
    /// through `DdnsService`. Per `.agents/auth.md` rule 3, a background task must
    /// still establish an admin [`AuthContext`] around its service/repository work,
    /// so the whole body runs under `AuthContext::system()` —
    /// mirroring `ddns::runner`/`tls::runner`.
    async fn resolve(&self) -> Option<(Arc<DaemonIdentity>, String, u16)> {
        let admin_ctx = AuthContext::system();
        auth_context::with_context(admin_ctx, async {
            let slug = self.system_config.get(KEY_REGION).await.ok().flatten()?;
            // Distinguish "misconfigured" from "not enrolled": a persisted region
            // the catalog cannot resolve means the tunnel will never come up, and
            // silence here previously made that state indistinguishable from a
            // box without remote access.
            let Some(gateway) = self
                .region_catalog
                .iter()
                .find(|entry| entry.slug == slug)
                .map(|entry| entry.gateway_base_url.clone())
            else {
                tracing::warn!(
                    region = %slug,
                    "tunnel runner: persisted DDNS region {slug} is not in this \
                     build's region catalog — the reverse tunnel cannot connect \
                     (update wardnet or re-register remote access)",
                    slug = slug,
                );
                return None;
            };
            let seed_bytes = self.secrets.get(SECRET_DAEMON_KEY).await.ok().flatten()?;
            let seed: [u8; 32] = seed_bytes.try_into().ok()?;
            let identity =
                DaemonIdentity::from_seed(seed, self.tenants.clone(), self.entitlement.clone());
            let port = self.system_config.inbound_wg_listen_port().await.ok()?;
            Some((identity, gateway, port))
        })
        .await
    }

    /// Re-read the currently-configured inbound-`WireGuard` listen port. Called once
    /// per new `conn_id` (not per datagram, so it is cheap) so that a live change to
    /// `inbound_wg_listen_port` takes effect for every NEW relayed flow without
    /// waiting for the WS connection to drop and reconnect. Returns [`None`] on a
    /// config-read error, letting the caller fall back to the port captured at
    /// connection start.
    async fn listen_port(&self) -> Option<u16> {
        self.system_config.inbound_wg_listen_port().await.ok()
    }
}

/// Handle to the spawned reverse-tunnel task. See [module docs](self).
pub struct TunnelerRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl TunnelerRunner {
    /// Start the runner under a child span of `parent`.
    #[must_use]
    pub fn start(connector: Arc<TunnelerConnector>, parent: &tracing::Span) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "tunneller_runner");
        let handle = tokio::spawn(runner_loop(connector, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the runner and wait for the task (and its connection) to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("reverse tunnel runner shut down");
    }
}

/// The outer lifecycle loop: gate → resolve → connect → serve → backoff, forever
/// until cancelled.
async fn runner_loop(connector: Arc<TunnelerConnector>, cancel: CancellationToken) {
    let mut backoff = Backoff::new(INITIAL_BACKOFF, MAX_BACKOFF);

    loop {
        if cancel.is_cancelled() {
            break;
        }

        // Gate: do nothing at all while the inbound server is off.
        if !connector.enabled().await {
            backoff.reset();
            if sleep_or_cancel(GATE_POLL_INTERVAL, &cancel).await {
                break;
            }
            continue;
        }

        // Not-yet-enrolled behaves like disabled: idle and retry.
        let Some((identity, gateway, port)) = connector.resolve().await else {
            backoff.reset();
            if sleep_or_cancel(GATE_POLL_INTERVAL, &cancel).await {
                break;
            }
            continue;
        };

        let client = TunnelerClient::new(gateway);
        let started = Instant::now();
        match client.connect(&identity).await {
            Ok(stream) => {
                tracing::info!("reverse tunnel connected");
                run_connection(stream, &connector, port, &cancel).await;
                // A connection that lasted long enough is evidence the endpoint is
                // healthy: retry the next drop from the floor, not a grown delay.
                if started.elapsed() >= STABLE_CONNECTION {
                    backoff.reset();
                }
            }
            Err(error) => {
                tracing::warn!(%error, "reverse tunnel connect failed");
            }
        }

        if cancel.is_cancelled() {
            break;
        }
        let delay = backoff.next();
        tracing::debug!(?delay, "reverse tunnel reconnecting after backoff");
        if sleep_or_cancel(delay, &cancel).await {
            break;
        }
    }

    tracing::info!("reverse tunnel runner loop exited");
}

/// One live tunnel connection. Multiplexes inbound frames, the outbound frame
/// queue, idle cleanup, and the disable-gate re-check over a single `select!` until
/// the WS ends, the server is disabled, or the runner is cancelled. On exit every
/// per-`conn_id` reader task is cancelled and the WS is dropped.
async fn run_connection(
    stream: TunnelStream,
    connector: &TunnelerConnector,
    port: u16,
    cancel: &CancellationToken,
) {
    let (mut ws_tx, mut ws_rx) = stream.split();
    // Single outbound funnel — the inbound handler and every per-conn reader task
    // push `Message`s here; only this loop writes the sink.
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);
    let mut conns: HashMap<u32, Conn> = HashMap::new();

    let mut cleanup = tokio::time::interval(CLEANUP_INTERVAL);
    cleanup.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut disable_check = tokio::time::interval(DISABLE_CHECK_INTERVAL);
    disable_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,

            inbound = ws_rx.next() => {
                let Some(message) = inbound else { break };
                let message = match message {
                    Ok(message) => message,
                    Err(error) => {
                        tracing::debug!(%error, "reverse tunnel WS read error");
                        break;
                    }
                };
                match message {
                    Message::Binary(data) => {
                        handle_frame(
                            &data,
                            connector,
                            port,
                            &out_tx,
                            &mut conns,
                            &open_relay,
                            &open_tcp_relay,
                        )
                        .await;
                    }
                    // Answer the WS-level ping explicitly so keepalive never stalls
                    // behind the transport's poll-driven auto-pong.
                    Message::Ping(payload) => {
                        let _ = out_tx.send(Message::Pong(payload)).await;
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }

            outbound = out_rx.recv() => {
                // `out_tx` is held for the loop's lifetime, so this never yields None.
                let Some(message) = outbound else { break };
                if ws_tx.send(message).await.is_err() {
                    break;
                }
            }

            _ = cleanup.tick() => {
                let now = Instant::now();
                let expired: Vec<u32> = conns
                    .iter()
                    .filter(|(_, conn)| now >= conn.deadline)
                    .map(|(id, _)| *id)
                    .collect();
                for conn_id in expired {
                    if let Some(conn) = conns.remove(&conn_id) {
                        conn.cancel.cancel();
                    }
                    let _ = out_tx
                        .send(Message::Binary(tunneller::encode_close(conn_id).into()))
                        .await;
                    tracing::debug!(conn_id, "reverse tunnel conn idle, closing");
                }
            }

            _ = disable_check.tick() => {
                if !connector.enabled().await {
                    tracing::info!("inbound WireGuard disabled, tearing down reverse tunnel");
                    break;
                }
            }
        }
    }

    // Teardown: stop every per-conn reader task; dropping the split halves closes
    // the WebSocket.
    for (_, conn) in conns {
        conn.cancel.cancel();
    }
}

/// The local end of a relayed flow — the transport-specific handle the inbound
/// `FRAME_DATA` path writes to.
pub(crate) enum ConnKind {
    /// Inbound `WireGuard`: a loopback UDP socket connected to the daemon's own
    /// inbound-WG port; datagrams are written straight to it.
    Udp(Arc<UdpSocket>),
    /// Private DNS: the write side of the loopback `DoT` TCP connection, reached
    /// through the relay task's channel (the task owns the split stream).
    Tcp(mpsc::Sender<Vec<u8>>),
}

impl ConnKind {
    /// The idle window for this transport, used to (re)arm a flow's deadline.
    fn idle_timeout(&self) -> Duration {
        match self {
            ConnKind::Udp(_) => UDP_IDLE_TIMEOUT,
            ConnKind::Tcp(_) => TCP_IDLE_TIMEOUT,
        }
    }
}

/// One active relayed flow: the local-end handle, its idle deadline, and the cancel
/// handle stopping its return-path task.
pub(crate) struct Conn {
    kind: ConnKind,
    deadline: Instant,
    cancel: CancellationToken,
}

#[cfg(test)]
impl Conn {
    /// Build a UDP `Conn` around an already-bound socket for unit tests, with no
    /// return-path reader spawned and a fresh cancel token.
    pub(crate) fn new_for_test(socket: Arc<UdpSocket>) -> Self {
        Self {
            kind: ConnKind::Udp(socket),
            deadline: Instant::now() + UDP_IDLE_TIMEOUT,
            cancel: CancellationToken::new(),
        }
    }

    /// Build a TCP `Conn` around a write-channel sender for unit tests.
    pub(crate) fn new_tcp_for_test(tx: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            kind: ConnKind::Tcp(tx),
            deadline: Instant::now() + TCP_IDLE_TIMEOUT,
            cancel: CancellationToken::new(),
        }
    }
}

/// Send a `FRAME_CLOSE` for `conn_id` on the outbound funnel — the reject/teardown
/// signal shared by the cap, gate, open-failure, and data-failure paths so a node
/// never hangs waiting on a relay that will not (or no longer) exist.
async fn send_close(out_tx: &mpsc::Sender<Message>, conn_id: u32) {
    let _ = out_tx
        .send(Message::Binary(tunneller::encode_close(conn_id).into()))
        .await;
}

/// Count the live flows of a given transport, so each cap is enforced against its
/// own kind rather than the combined total.
fn count_kind(conns: &HashMap<u32, Conn>, tcp: bool) -> usize {
    conns
        .values()
        .filter(|conn| matches!(conn.kind, ConnKind::Tcp(_)) == tcp)
        .count()
}

/// Dispatch one inbound binary frame.
///
/// `open_udp` / `open_tcp` are the relay openers — production passes [`open_relay`]
/// and [`open_tcp_relay`]; tests inject fakes to force failures, record the resolved
/// port, or stand in for a real socket without touching the network. `fallback_port`
/// is the inbound-WG port captured at connection start, used only when the live
/// re-read of `inbound_wg_listen_port` fails.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_frame<F, Fut, G, Gut>(
    data: &[u8],
    connector: &TunnelerConnector,
    fallback_port: u16,
    out_tx: &mpsc::Sender<Message>,
    conns: &mut HashMap<u32, Conn>,
    open_udp: &F,
    open_tcp: &G,
) where
    F: Fn(u32, u16, mpsc::Sender<Message>) -> Fut,
    Fut: Future<Output = std::io::Result<Conn>>,
    G: Fn(u32, mpsc::Sender<Message>) -> Gut,
    Gut: Future<Output = std::io::Result<Conn>>,
{
    let Some(frame) = tunneller::decode(data) else {
        return;
    };
    match frame {
        Frame::Ping => {
            let _ = out_tx
                .send(Message::Binary(tunneller::encode_pong().into()))
                .await;
        }
        Frame::Connect { conn_id, dest_port } => {
            if conns.contains_key(&conn_id) {
                // Duplicate CONNECT for a live conn_id — keep the existing relay.
                return;
            }
            // The path is selected by `dest_port`: Private DNS terminates the
            // cloud's `:853` SNI passthrough here, HTTPS is reserved for #816, and
            // everything else is the inbound-WireGuard UDP relay (whose `dest_port`
            // stays advisory — see module docs).
            match dest_port {
                DOT_PORT => {
                    connect_tcp(conn_id, connector, out_tx, conns, open_tcp).await;
                }
                // deferred to #816: no local `:443` target exists yet, so close it.
                HTTPS_PORT => send_close(out_tx, conn_id).await,
                _ => {
                    connect_udp(conn_id, connector, fallback_port, out_tx, conns, open_udp).await;
                }
            }
        }
        Frame::Data { conn_id, payload } => {
            let failed = match conns.get_mut(&conn_id) {
                Some(conn) => {
                    let sent = match &conn.kind {
                        ConnKind::Udp(socket) => socket.send(&payload).await.is_ok(),
                        ConnKind::Tcp(tx) => tx.send(payload).await.is_ok(),
                    };
                    if sent {
                        conn.deadline = Instant::now() + conn.kind.idle_timeout();
                        false
                    } else {
                        true
                    }
                }
                // No CONNECT seen for this conn_id — tolerate and ignore.
                None => false,
            };
            if failed {
                if let Some(conn) = conns.remove(&conn_id) {
                    conn.cancel.cancel();
                }
                send_close(out_tx, conn_id).await;
            }
        }
        Frame::Close { conn_id } => {
            if let Some(conn) = conns.remove(&conn_id) {
                conn.cancel.cancel();
            }
        }
    }
}

/// Open the inbound-`WireGuard` UDP relay for a new `conn_id`, gated on the WG
/// feature and its own [`MAX_UDP_CONNS`] cap.
async fn connect_udp<F, Fut>(
    conn_id: u32,
    connector: &TunnelerConnector,
    fallback_port: u16,
    out_tx: &mpsc::Sender<Message>,
    conns: &mut HashMap<u32, Conn>,
    open_udp: &F,
) where
    F: Fn(u32, u16, mpsc::Sender<Message>) -> Fut,
    Fut: Future<Output = std::io::Result<Conn>>,
{
    // Per-frame gate: reject WG flows while the feature is off, even if the tunnel
    // is up only for Private DNS.
    if !connector.wg_enabled().await {
        send_close(out_tx, conn_id).await;
        return;
    }
    if count_kind(conns, false) >= MAX_UDP_CONNS {
        tracing::warn!(
            conn_id,
            max = MAX_UDP_CONNS,
            "reverse tunnel: at MAX_UDP_CONNS cap, rejecting new UDP conn_id"
        );
        send_close(out_tx, conn_id).await;
        return;
    }
    // Re-read the configured inbound-WG port so a live port change takes effect for
    // every NEW flow. Already-open relay sockets keep their original target until
    // they close or idle out (documented limitation).
    let port = connector.listen_port().await.unwrap_or(fallback_port);
    match open_udp(conn_id, port, out_tx.clone()).await {
        Ok(conn) => {
            conns.insert(conn_id, conn);
        }
        Err(error) => {
            tracing::warn!(conn_id, %error, "reverse tunnel: failed to open local UDP relay socket");
            send_close(out_tx, conn_id).await;
        }
    }
}

/// Open the Private-DNS TCP relay for a new `conn_id`, gated on the Private-DNS
/// feature and its own [`MAX_TCP_CONNS`] cap. The opener emits `FRAME_READY` on a
/// successful connect (before any data), so nothing extra is sent here.
async fn connect_tcp<G, Gut>(
    conn_id: u32,
    connector: &TunnelerConnector,
    out_tx: &mpsc::Sender<Message>,
    conns: &mut HashMap<u32, Conn>,
    open_tcp: &G,
) where
    G: Fn(u32, mpsc::Sender<Message>) -> Gut,
    Gut: Future<Output = std::io::Result<Conn>>,
{
    if !connector.private_dns_enabled().await {
        send_close(out_tx, conn_id).await;
        return;
    }
    if count_kind(conns, true) >= MAX_TCP_CONNS {
        tracing::warn!(
            conn_id,
            max = MAX_TCP_CONNS,
            "reverse tunnel: at MAX_TCP_CONNS cap, rejecting new DoT conn_id"
        );
        send_close(out_tx, conn_id).await;
        return;
    }
    match open_tcp(conn_id, out_tx.clone()).await {
        Ok(conn) => {
            conns.insert(conn_id, conn);
        }
        Err(error) => {
            tracing::warn!(conn_id, %error, "reverse tunnel: failed to open local DoT relay");
            send_close(out_tx, conn_id).await;
        }
    }
}

/// Open a loopback UDP socket connected to `127.0.0.1:<port>` and spawn its
/// return-path reader (local socket → `FRAME_DATA` → `out_tx`).
async fn open_relay(
    conn_id: u32,
    port: u16,
    out_tx: mpsc::Sender<Message>,
) -> std::io::Result<Conn> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    socket.connect((Ipv4Addr::LOCALHOST, port)).await?;
    let socket = Arc::new(socket);
    let cancel = CancellationToken::new();
    let reader_socket = Arc::clone(&socket);
    let reader_cancel = cancel.clone();
    let span = tracing::Span::current();
    tokio::spawn(udp_reader(conn_id, reader_socket, out_tx, reader_cancel).instrument(span));
    Ok(Conn {
        kind: ConnKind::Udp(socket),
        deadline: Instant::now() + UDP_IDLE_TIMEOUT,
        cancel,
    })
}

/// Open the loopback `DoT` TCP relay for `conn_id`, targeting the daemon's own
/// `:853` listener. Thin wrapper over [`connect_dot_relay`] pinning [`DOT_PORT`] so
/// no frame-carried port can redirect the relay.
async fn open_tcp_relay(conn_id: u32, out_tx: mpsc::Sender<Message>) -> std::io::Result<Conn> {
    connect_dot_relay(conn_id, DOT_PORT, out_tx).await
}

/// Connect `127.0.0.1:<port>` (bounded by [`TCP_CONNECT_TIMEOUT`]), emit
/// `FRAME_READY` the instant the connect lands, and spawn the bidirectional relay
/// task. A connect error/timeout returns `Err`, which the caller turns into a
/// `FRAME_CLOSE`. Parameterised on `port` so tests can drive it against a loopback
/// echo; production always calls it through [`open_tcp_relay`] at [`DOT_PORT`].
pub(crate) async fn connect_dot_relay(
    conn_id: u32,
    port: u16,
    out_tx: mpsc::Sender<Message>,
) -> std::io::Result<Conn> {
    let stream = tokio::time::timeout(
        TCP_CONNECT_TIMEOUT,
        TcpStream::connect((Ipv4Addr::LOCALHOST, port)),
    )
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "DoT connect timed out"))??;

    // FRAME_READY must precede any relayed byte: queue it on the ordered funnel
    // *before* the relay task can push a FRAME_DATA, so the node sees "local end up"
    // first exactly as the cloud protocol requires.
    let _ = out_tx
        .send(Message::Binary(tunneller::encode_ready(conn_id).into()))
        .await;

    // The relay task owns the split stream; inbound FRAME_DATA reaches its write half
    // through this channel.
    let (write_tx, write_rx) = mpsc::channel::<Vec<u8>>(256);
    let cancel = CancellationToken::new();
    let reader_cancel = cancel.clone();
    let span = tracing::Span::current();
    tokio::spawn(tcp_relay(conn_id, stream, write_rx, out_tx, reader_cancel).instrument(span));
    Ok(Conn {
        kind: ConnKind::Tcp(write_tx),
        deadline: Instant::now() + TCP_IDLE_TIMEOUT,
        cancel,
    })
}

/// Bidirectional relay for one `DoT` `conn_id`: local socket → `FRAME_DATA` → node,
/// and node payloads (arriving on `write_rx`) → local socket, until cancelled, either
/// side EOFs/errors, or the funnel closes. On local EOF it sends a `FRAME_CLOSE` so
/// the node tears its half down.
async fn tcp_relay(
    conn_id: u32,
    stream: TcpStream,
    mut write_rx: mpsc::Receiver<Vec<u8>>,
    out_tx: mpsc::Sender<Message>,
    cancel: CancellationToken,
) {
    let (mut read_half, mut write_half) = stream.into_split();
    // DoT frames are small (a length-prefixed DNS message), but 16 KiB keeps large
    // TCP segments in one read without over-allocating per connection.
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,

            // node → local: write each relayed payload to the DoT listener.
            payload = write_rx.recv() => {
                // `None` means the Conn (and its write_tx) was dropped — teardown.
                let Some(payload) = payload else { break };
                if write_half.write_all(&payload).await.is_err() {
                    break;
                }
            }

            // local → node: forward each segment the DoT listener sends back.
            result = read_half.read(&mut buf) => match result {
                Ok(0) => {
                    // Local EOF: tell the node the flow is done, then stop.
                    let _ = out_tx
                        .send(Message::Binary(tunneller::encode_close(conn_id).into()))
                        .await;
                    break;
                }
                Ok(n) => {
                    let frame = tunneller::encode_data(conn_id, &buf[..n]);
                    if out_tx.send(Message::Binary(frame.into())).await.is_err() {
                        break;
                    }
                }
                Err(error) => {
                    tracing::debug!(conn_id, %error, "reverse tunnel local DoT socket read error");
                    break;
                }
            },
        }
    }
}

/// Return-path reader for one `conn_id`: forwards each datagram the local inbound-WG
/// server sends back as a `FRAME_DATA`, until cancelled or the socket errors.
async fn udp_reader(
    conn_id: u32,
    socket: Arc<UdpSocket>,
    out_tx: mpsc::Sender<Message>,
    cancel: CancellationToken,
) {
    // 64 KiB is the hard UDP payload ceiling — covers any WireGuard datagram.
    let mut buf = vec![0u8; 65_535];
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = socket.recv(&mut buf) => match result {
                Ok(n) => {
                    let frame = tunneller::encode_data(conn_id, &buf[..n]);
                    if out_tx.send(Message::Binary(frame.into())).await.is_err() {
                        break;
                    }
                }
                Err(error) => {
                    tracing::debug!(conn_id, %error, "reverse tunnel local relay socket read error");
                    break;
                }
            },
        }
    }
}

/// Sleep for `delay`, or return early if the runner is cancelled. Returns `true`
/// when cancellation won the race (the caller should stop).
async fn sleep_or_cancel(delay: Duration, cancel: &CancellationToken) -> bool {
    tokio::select! {
        () = cancel.cancelled() => true,
        () = tokio::time::sleep(delay) => false,
    }
}
