//! Background runner owning the daemon's **one persistent connection**: the
//! reverse tunnel to wardnet-cloud's Tunneller (cloud ADR-0015, `wardnet`
//! ADR-0022, issue #809).
//!
//! This is genuinely new architecture for the daemon — every other background
//! component (`ddns::runner`, `tls::runner`, …) is a fixed-interval poll loop with
//! no long-lived socket. Here the runner:
//!
//! 1. **Gates** on `inbound_wg_enabled()` — it never dials while the inbound
//!    `WireGuard` server is off, and tears the connection (and every local relay
//!    socket) down promptly if the server is disabled mid-session.
//! 2. **Dials** via [`TunnelerClient`], reusing the same enrollment identity + region
//!    the DDNS client uses (seed + `ddns_region` slug the wizard persisted).
//! 3. **Relays** each `conn_id`: on `FRAME_CONNECT` it opens a loopback UDP socket
//!    to the daemon's *own* `inbound_wg_listen_port` (deliberately **ignoring** the
//!    frame's `dest_port`, which the cloud currently hard-codes to a placeholder —
//!    see its `TODO(wardnet#809)`; trusting a port the daemon didn't configure would
//!    be a local-relay-target-confusion risk), spawns a reader task for the return
//!    path, and forwards `FRAME_DATA` both ways until `FRAME_CLOSE` or the NAT-style
//!    idle timeout.
//! 4. **Reconnects** with exponential backoff (1s → ×2 → 60s cap), reset after a
//!    connection stays up past [`STABLE_CONNECTION`].
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
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use uuid::Uuid;
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
/// Idle window after which a relayed `conn_id` is torn down and `FRAME_CLOSE` sent
/// proactively. Mirrors `wardnet-cloud`'s `UDP_IDLE_TIMEOUT`: UDP/`WireGuard` has no
/// explicit teardown, so this NAT-style timeout (2 min) comfortably outlasts several
/// missed keepalives (default 25s) before reclaiming a live-but-quiet peer.
const UDP_IDLE_TIMEOUT: Duration = Duration::from_mins(2);

/// Upper bound on concurrently relayed `conn_id`s over one WS connection. Mirrors
/// `wardnet-cloud`'s own `UDP_MAX_CONNS = 8`
/// (`source/crates/tunneller/src/tunnel/udp_relay.rs`) as **defense-in-depth**: the
/// cloud already caps its side, and the daemon independently caps its own so a
/// misbehaving or compromised node cannot make it open an unbounded number of local
/// relay sockets before the 2-minute idle sweep reclaims them.
const MAX_CONNS: usize = 8;

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

    /// Whether the inbound `WireGuard` server is enabled — the runner's gate.
    async fn enabled(&self) -> bool {
        match self.system_config.inbound_wg_enabled().await {
            Ok(enabled) => enabled,
            Err(error) => {
                tracing::warn!(%error, "reverse tunnel: failed to read inbound-wg enabled flag");
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
    /// so the whole body runs under `AuthContext::Admin { admin_id: nil }` —
    /// mirroring `ddns::runner`/`tls::runner`.
    async fn resolve(&self) -> Option<(Arc<DaemonIdentity>, String, u16)> {
        let admin_ctx = AuthContext::Admin {
            admin_id: Uuid::nil(),
        };
        auth_context::with_context(admin_ctx, async {
            let slug = self.system_config.get(KEY_REGION).await.ok().flatten()?;
            let gateway = self
                .region_catalog
                .iter()
                .find(|entry| entry.slug == slug)
                .map(|entry| entry.gateway_base_url.clone())?;
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
                        handle_frame(&data, connector, port, &out_tx, &mut conns, &open_relay).await;
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

/// One active relayed flow: the loopback socket connected to the daemon's inbound
/// `WireGuard` port, its NAT-style idle deadline, and the cancel handle stopping its
/// return-path reader task.
pub(crate) struct Conn {
    socket: Arc<UdpSocket>,
    deadline: Instant,
    cancel: CancellationToken,
}

#[cfg(test)]
impl Conn {
    /// Build a `Conn` around an already-bound socket for unit tests, with no
    /// return-path reader spawned and a fresh cancel token.
    pub(crate) fn new_for_test(socket: Arc<UdpSocket>) -> Self {
        Self {
            socket,
            deadline: Instant::now() + UDP_IDLE_TIMEOUT,
            cancel: CancellationToken::new(),
        }
    }
}

/// Dispatch one inbound binary frame.
///
/// `open` is the relay-socket opener — production passes [`open_relay`]; tests inject
/// a fake to force failures or record the resolved port without touching the network.
/// `fallback_port` is the port captured at connection start, used only when the live
/// re-read of `inbound_wg_listen_port` fails.
pub(crate) async fn handle_frame<F, Fut>(
    data: &[u8],
    connector: &TunnelerConnector,
    fallback_port: u16,
    out_tx: &mpsc::Sender<Message>,
    conns: &mut HashMap<u32, Conn>,
    open: &F,
) where
    F: Fn(u32, u16, mpsc::Sender<Message>) -> Fut,
    Fut: Future<Output = std::io::Result<Conn>>,
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
        // `dest_port` is intentionally ignored — always relay to the daemon's own
        // configured inbound-WG port (see module docs).
        Frame::Connect {
            conn_id,
            dest_port: _,
        } => {
            if conns.contains_key(&conn_id) {
                // Duplicate CONNECT for a live conn_id — keep the existing relay.
                return;
            }
            // Defense-in-depth: cap the number of live flows independently of the
            // cloud's own `UDP_MAX_CONNS`. Reject a new conn_id at the ceiling and
            // tell the node with a FRAME_CLOSE (as the idle sweep + data-failure
            // paths do) so it doesn't silently hang waiting for a relay that will
            // never open.
            if conns.len() >= MAX_CONNS {
                tracing::warn!(
                    conn_id,
                    max = MAX_CONNS,
                    "reverse tunnel: at MAX_CONNS cap, rejecting new conn_id"
                );
                let _ = out_tx
                    .send(Message::Binary(tunneller::encode_close(conn_id).into()))
                    .await;
                return;
            }
            // Re-read the configured inbound-WG port so a live port change takes
            // effect for every NEW flow. Already-open relay sockets keep their
            // original target until they close or idle out (documented limitation).
            let port = connector.listen_port().await.unwrap_or(fallback_port);
            match open(conn_id, port, out_tx.clone()).await {
                Ok(conn) => {
                    conns.insert(conn_id, conn);
                }
                Err(error) => {
                    tracing::warn!(conn_id, %error, "reverse tunnel: failed to open local relay socket");
                    // Tell the node the flow was rejected instead of leaving it to
                    // hang (matching the cap + data-failure FRAME_CLOSE paths).
                    let _ = out_tx
                        .send(Message::Binary(tunneller::encode_close(conn_id).into()))
                        .await;
                }
            }
        }
        Frame::Data { conn_id, payload } => {
            let failed = if let Some(conn) = conns.get_mut(&conn_id) {
                if conn.socket.send(&payload).await.is_ok() {
                    conn.deadline = Instant::now() + UDP_IDLE_TIMEOUT;
                    false
                } else {
                    true
                }
            } else {
                // No CONNECT seen for this conn_id — tolerate and ignore.
                false
            };
            if failed {
                if let Some(conn) = conns.remove(&conn_id) {
                    conn.cancel.cancel();
                }
                let _ = out_tx
                    .send(Message::Binary(tunneller::encode_close(conn_id).into()))
                    .await;
            }
        }
        Frame::Close { conn_id } => {
            if let Some(conn) = conns.remove(&conn_id) {
                conn.cancel.cancel();
            }
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
        socket,
        deadline: Instant::now() + UDP_IDLE_TIMEOUT,
        cancel,
    })
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
