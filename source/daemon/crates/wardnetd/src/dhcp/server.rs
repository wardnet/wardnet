use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use dhcproto::v4::{DhcpOption, Flags, HType, Message, MessageType, Opcode, OptionCode};
use dhcproto::{Decodable, Decoder, Encodable, Encoder};
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use wardnet_common::auth::AuthContext;
use wardnet_common::device::DeviceSignalKind;
use wardnet_common::dhcp::{DhcpLease, DhcpScope};
use wardnetd_services::auth_context;
use wardnetd_services::device::DeviceIdentificationService;
use wardnetd_services::dhcp::DhcpService;
use wardnetd_services::dhcp::server::{DhcpServer, DhcpSocket};
use wardnetd_services::error::AppError;

// ---------------------------------------------------------------------------
// UdpDhcpSocket — production socket impl
// ---------------------------------------------------------------------------

/// Production [`DhcpSocket`] backed by a real tokio UDP socket.
pub struct UdpDhcpSocket {
    socket: UdpSocket,
}

impl UdpDhcpSocket {
    /// Bind a UDP socket with broadcast enabled.
    pub async fn bind(addr: SocketAddr) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(addr).await?;
        socket.set_broadcast(true)?;
        Ok(Self { socket })
    }

    /// Return the local address of the bound socket.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

#[async_trait]
impl DhcpSocket for UdpDhcpSocket {
    async fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        self.socket.recv_from(buf).await
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> std::io::Result<usize> {
        self.socket.send_to(buf, target).await
    }
}

// ---------------------------------------------------------------------------
// UdpDhcpServer
// ---------------------------------------------------------------------------

/// Production DHCP server that processes DISCOVER/REQUEST/RELEASE
/// messages using the service layer.
pub struct UdpDhcpServer {
    /// Service for lease management.
    service: Arc<dyn DhcpService>,
    /// Records DHCP options 55/60 as identification signals (issue #1099).
    ///
    /// Optional so tests that only exercise the lease protocol can leave it
    /// out; production always supplies it. Recording is best-effort — a failed
    /// signal write must never fail the lease.
    identification: Option<Arc<dyn DeviceIdentificationService>>,
    /// Address to bind the UDP socket to.
    bind_addr: SocketAddr,
    /// Pre-injected socket (used in tests). When `None`, `start()` binds a new one.
    injected_socket: Option<Arc<dyn DhcpSocket>>,
    /// Whether the server loop is actively running.
    running: Arc<AtomicBool>,
    /// Cancellation token for the server loop, replaced on each `start()`.
    cancel: Mutex<CancellationToken>,
    /// Handle to the spawned server task.
    handle: Mutex<Option<JoinHandle<()>>>,
    /// The actual local address after binding (useful for ephemeral ports).
    local_addr: Arc<std::sync::Mutex<Option<SocketAddr>>>,
}

impl UdpDhcpServer {
    /// Create a new DHCP server that binds to `0.0.0.0:67` (the standard DHCP port).
    #[must_use]
    pub fn new(service: Arc<dyn DhcpService>) -> Self {
        Self::with_bind_addr(service, SocketAddr::from(([0, 0, 0, 0], 67)))
    }

    /// Attach the identification service that receives DHCP options 55/60.
    ///
    /// Builder-style rather than a constructor parameter so the many existing
    /// test constructions stay untouched.
    #[must_use]
    pub fn with_identification(
        mut self,
        identification: Arc<dyn DeviceIdentificationService>,
    ) -> Self {
        self.identification = Some(identification);
        self
    }

    /// Create a new DHCP server that binds to the given address.
    ///
    /// Use `127.0.0.1:0` in tests so the OS assigns an ephemeral port and
    /// the server operates entirely over loopback.
    #[must_use]
    pub(crate) fn with_bind_addr(service: Arc<dyn DhcpService>, bind_addr: SocketAddr) -> Self {
        Self {
            service,
            bind_addr,
            injected_socket: None,
            running: Arc::new(AtomicBool::new(false)),
            cancel: Mutex::new(CancellationToken::new()),
            handle: Mutex::new(None),
            identification: None,
            local_addr: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Create a DHCP server with a pre-bound socket (for testing).
    ///
    /// The socket is used directly instead of binding a new one in `start()`.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_socket(service: Arc<dyn DhcpService>, socket: Arc<dyn DhcpSocket>) -> Self {
        Self {
            service,
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 0)),
            injected_socket: Some(socket),
            running: Arc::new(AtomicBool::new(false)),
            cancel: Mutex::new(CancellationToken::new()),
            handle: Mutex::new(None),
            identification: None,
            local_addr: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Return the actual local address the server is bound to, if it has started.
    ///
    /// Useful in tests when binding to port 0 to discover the ephemeral port.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn local_addr(&self) -> Option<SocketAddr> {
        *self.local_addr.lock().expect("local_addr mutex poisoned")
    }
}

#[async_trait]
impl DhcpServer for UdpDhcpServer {
    async fn start(&self) -> Result<(), AppError> {
        if self.running.load(Ordering::SeqCst) {
            tracing::warn!("DHCP server already running");
            return Ok(());
        }

        let socket: Arc<dyn DhcpSocket> = if let Some(ref s) = self.injected_socket {
            Arc::clone(s)
        } else {
            let udp_socket = UdpDhcpSocket::bind(self.bind_addr).await.map_err(|e| {
                AppError::Internal(anyhow::anyhow!(
                    "failed to bind DHCP socket on {}: {e}",
                    self.bind_addr
                ))
            })?;

            let actual_addr = udp_socket.local_addr().map_err(|e| {
                AppError::Internal(anyhow::anyhow!("failed to get local addr: {e}"))
            })?;

            // Store the actual address so tests can discover the ephemeral port.
            if let Ok(mut guard) = self.local_addr.lock() {
                *guard = Some(actual_addr);
            }

            tracing::info!(%actual_addr, "DHCP server listening on {actual_addr}");

            Arc::new(udp_socket)
        };

        let service = Arc::clone(&self.service);
        let identification = self.identification.clone();
        let running = Arc::clone(&self.running);

        // The daemon's own interfaces (including ones other than the LAN
        // interface, e.g. an idle secondary NIC) must never be leased —
        // otherwise a DISCOVER that loops back onto the LAN from the host's
        // own hardware gets a real lease and shows up as a phantom device
        // (mirrors the packet-capture self-filter in `packet_capture_pnet`).
        let own_macs: Arc<HashSet<String>> = Arc::new(
            crate::packet_capture_pnet::local_mac_addresses()
                .into_iter()
                .map(crate::packet_capture_pnet::format_mac)
                .collect(),
        );

        // Create a fresh cancellation token so stop()/start() cycles work.
        let new_cancel = CancellationToken::new();
        let cancel = new_cancel.clone();
        *self.cancel.lock().await = new_cancel;

        running.store(true, Ordering::SeqCst);

        let handle = tokio::spawn(async move {
            server_loop(
                socket,
                service,
                identification,
                running.clone(),
                cancel,
                own_macs,
            )
            .await;
            running.store(false, Ordering::SeqCst);
            tracing::info!("DHCP server loop exited");
        });

        *self.handle.lock().await = Some(handle);
        Ok(())
    }

    async fn stop(&self) -> Result<(), AppError> {
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.cancel.lock().await.cancel();

        if let Some(handle) = self.handle.lock().await.take() {
            let _ = handle.await;
        }

        tracing::info!("DHCP server stopped");
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

/// Upper bound on DHCP handlers running at once.
///
/// Sized for a home LAN's client count with room to spare, not for the traffic
/// an attacker can generate: port 67 takes unauthenticated broadcast, so a
/// spawn per packet with no ceiling is a memory-exhaustion path. Packets that
/// arrive with every permit taken are dropped, and DHCP clients retransmit.
const MAX_CONCURRENT_HANDLERS: usize = 64;

/// A packet parked behind the handler currently serving its MAC.
type PendingPacket = (MessageType, Message, SocketAddr);

/// Runs packet handlers concurrently across MACs while keeping each MAC's own
/// packets strictly in order.
///
/// The receive loop must never wait on lease work: it owns the only socket, so
/// a slow database call made inline stalls every client at once, and the
/// clients respond by retransmitting into a server that is already behind.
///
/// Serializing per MAC is not just tidiness. `assign_lease` and `renew_lease`
/// read a MAC's current lease and write it back, so two packets from one client
/// in flight together are two writers racing over one row — and a retransmitting
/// client produces exactly that. Each MAC therefore gets at most one running
/// handler plus one pending packet; a further packet replaces the pending one,
/// which collapses a retransmit burst into a single answer built from the
/// client's newest message.
struct Dispatcher {
    socket: Arc<dyn DhcpSocket>,
    service: Arc<dyn DhcpService>,
    identification: Option<Arc<dyn DeviceIdentificationService>>,
    /// MACs with a handler running. The value is that MAC's pending packet, if
    /// one arrived while the handler was busy. Held under a `std::sync::Mutex`
    /// because every critical section is a map lookup with no `.await` in it,
    /// and because [`InflightGuard`] has to release the slot from `Drop`.
    inflight: Arc<StdMutex<HashMap<String, Option<PendingPacket>>>>,
    permits: Arc<Semaphore>,
}

/// Releases a MAC's handler slot even if the handler panics.
///
/// Without this a panicking handler would leave the MAC marked as busy forever,
/// and that client would never be served again for the lifetime of the process.
struct InflightGuard {
    inflight: Arc<StdMutex<HashMap<String, Option<PendingPacket>>>>,
    mac: String,
    /// Cleared once the handler has released the slot itself.
    ///
    /// The handler's own exit removes the entry under the lock and then
    /// returns, dropping that lock before this guard runs. Removing a second
    /// time would delete an entry belonging to a handler dispatch had already
    /// started for the same MAC in between — and with no entry left, the next
    /// packet starts a third. Two handlers for one MAC is precisely what the
    /// dispatcher exists to prevent.
    armed: bool,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Ok(mut map) = self.inflight.lock() {
            map.remove(&self.mac);
        }
    }
}

impl Dispatcher {
    fn new(
        socket: Arc<dyn DhcpSocket>,
        service: Arc<dyn DhcpService>,
        identification: Option<Arc<dyn DeviceIdentificationService>>,
    ) -> Self {
        Self {
            socket,
            service,
            identification,
            inflight: Arc::new(StdMutex::new(HashMap::new())),
            permits: Arc::new(Semaphore::new(MAX_CONCURRENT_HANDLERS)),
        }
    }

    /// Wait until every in-flight handler has finished.
    ///
    /// Acquiring every permit is only possible once none are held, and
    /// `forget` keeps them out of circulation so nothing starts afterwards.
    async fn drain(&self) {
        let all = u32::try_from(MAX_CONCURRENT_HANDLERS).unwrap_or(u32::MAX);
        if let Ok(permits) = self.permits.acquire_many(all).await {
            permits.forget();
        }
    }

    /// Hand one packet off for handling. Returns immediately — never awaits the
    /// handler, so the caller can get straight back to `recv_from`.
    fn dispatch(&self, msg_type: MessageType, msg: Message, mac: String, src_addr: SocketAddr) {
        {
            let Ok(mut map) = self.inflight.lock() else {
                tracing::error!(%mac, "DHCP dispatch state poisoned, dropping packet");
                return;
            };
            if let Some(slot) = map.get_mut(&mac) {
                // A handler for this MAC is mid-flight. Keep only the newest
                // packet: an older queued one is a retransmit of a question this
                // client has already restated.
                //
                // A queued DHCPRELEASE is the exception — it is not a restated
                // question but a distinct instruction, and the DISCOVER a
                // rebooting client sends straight after would otherwise drop
                // it and leave the lease held.
                if let Some((queued_type, _, _)) = slot.as_ref() {
                    if *queued_type == MessageType::Release {
                        tracing::debug!(
                            %mac,
                            ?msg_type,
                            "keeping queued DHCPRELEASE, dropping the packet behind it"
                        );
                        return;
                    }
                    tracing::debug!(%mac, "superseding queued DHCP packet with a newer one");
                }
                *slot = Some((msg_type, msg, src_addr));
                return;
            }

            let Ok(permit) = Arc::clone(&self.permits).try_acquire_owned() else {
                tracing::warn!(
                    %mac,
                    ?msg_type,
                    max = MAX_CONCURRENT_HANDLERS,
                    "DHCP handler limit reached, dropping packet"
                );
                return;
            };

            map.insert(mac.clone(), None);

            let socket = Arc::clone(&self.socket);
            let service = Arc::clone(&self.service);
            let identification = self.identification.clone();
            let inflight = Arc::clone(&self.inflight);
            tokio::spawn(async move {
                let _permit = permit;
                let mut guard = InflightGuard {
                    inflight: Arc::clone(&inflight),
                    mac: mac.clone(),
                    armed: true,
                };
                let mut current = (msg_type, msg, src_addr);
                loop {
                    let (msg_type, msg, src_addr) = current;
                    handle_packet(
                        &socket,
                        &service,
                        identification.as_ref(),
                        msg_type,
                        &msg,
                        &mac,
                        src_addr,
                    )
                    .await;

                    // Take whatever arrived for this MAC while we worked. The
                    // slot is cleared and the MAC released in the same locked
                    // section, so a packet can never land between the two and
                    // be stranded with no handler to pick it up.
                    let Ok(mut map) = inflight.lock() else {
                        return;
                    };
                    let Some(next) = map.get_mut(&mac).and_then(Option::take) else {
                        map.remove(&mac);
                        guard.armed = false;
                        return;
                    };
                    current = next;
                }
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Server loop and helpers
// ---------------------------------------------------------------------------

/// Main server loop: receive DHCP packets, decode, dispatch, and respond.
pub(crate) async fn server_loop(
    socket: Arc<dyn DhcpSocket>,
    service: Arc<dyn DhcpService>,
    identification: Option<Arc<dyn DeviceIdentificationService>>,
    running: Arc<AtomicBool>,
    cancel: CancellationToken,
    own_macs: Arc<HashSet<String>>,
) {
    let dispatcher = Dispatcher::new(socket, service, identification);
    let mut buf = vec![0u8; 1500];

    loop {
        let (len, src_addr) = tokio::select! {
            () = cancel.cancelled() => break,
            result = dispatcher.socket.recv_from(&mut buf) => {
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!(error = %e, "DHCP socket recv error: {e}");
                        continue;
                    }
                }
            }
        };

        let packet = &buf[..len];
        let msg = match Message::decode(&mut Decoder::new(packet)) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(error = %e, "failed to decode DHCP message: {e}");
                continue;
            }
        };

        // A legitimate DHCP client on this network is always Ethernet or
        // Wi-Fi: htype 1 with a 6-byte hardware address. Enforcing that
        // before any `chaddr()` call bounds the wire-supplied `hlen` —
        // `dhcproto` slices its fixed 16-byte chaddr array by `hlen`, so an
        // attacker-controlled hlen > 16 would panic and silently kill the
        // whole server loop (issue #829) — and rejects degenerate identities
        // (hlen 0-5 would become truncated or empty MAC lease keys). This
        // guard also covers the `request.chaddr()` calls in
        // `build_response`/`build_nak`, which only ever see messages that
        // passed this loop.
        let htype = msg.htype();
        let hlen = msg.hlen();
        if htype != HType::Eth || hlen != 6 {
            tracing::debug!(
                ?htype,
                hlen,
                %src_addr,
                "dropping DHCP message with non-Ethernet hardware address: htype={htype:?}, hlen={hlen}, src={src_addr}"
            );
            continue;
        }

        let Some(msg_type) = msg.opts().msg_type() else {
            tracing::debug!("DHCP message has no message type option, ignoring");
            continue;
        };

        let mac = format_mac(msg.chaddr());
        tracing::debug!(%mac, ?msg_type, xid = msg.xid(), "received DHCP message: mac={mac}, type={msg_type:?}");

        if own_macs.contains(&mac) {
            tracing::debug!(
                %mac,
                ?msg_type,
                "ignoring DHCP message from the daemon's own interface: mac={mac}, type={msg_type:?}"
            );
            continue;
        }

        dispatcher.dispatch(msg_type, msg, mac, src_addr);
    }

    // Handlers run on their own tasks and keep using the socket and the lease
    // service after the loop breaks. Clearing `running` first would tell a
    // shutdown it is safe to tear those down mid-write, so wait until every
    // permit is back before saying the server has stopped.
    dispatcher.drain().await;
    running.store(false, Ordering::SeqCst);
}

/// Serve one DHCP message and send whatever it produces.
async fn handle_packet(
    socket: &Arc<dyn DhcpSocket>,
    service: &Arc<dyn DhcpService>,
    identification: Option<&Arc<dyn DeviceIdentificationService>>,
    msg_type: MessageType,
    msg: &Message,
    mac: &str,
    src_addr: SocketAddr,
) {
    match msg_type {
        MessageType::Discover => {
            let _ = record_dhcp_signals(identification, msg, mac);
            match handle_discover(service, msg, mac).await {
                Ok(response) => {
                    // DHCP OFFERs must be broadcast — the client has no IP yet
                    // and can only receive broadcast packets.
                    let broadcast = SocketAddr::from(([255, 255, 255, 255], 68));
                    send_response(socket.as_ref(), &response, broadcast).await;
                }
                Err(e) => {
                    tracing::error!(%mac, error = %e, "failed to handle DHCPDISCOVER for {mac}: {e}");
                }
            }
        }
        MessageType::Request => {
            let _ = record_dhcp_signals(identification, msg, mac);
            match handle_request(service, msg, mac).await {
                Ok(response) => {
                    let dest = reply_destination(msg, &response, src_addr);
                    send_response(socket.as_ref(), &response, dest).await;
                }
                Err(e) => {
                    tracing::error!(%mac, error = %e, "failed to handle DHCPREQUEST for {mac}: {e}");
                }
            }
        }
        MessageType::Release => {
            handle_release(service, msg, mac, src_addr).await;
        }
        other => {
            tracing::debug!(%mac, ?other, "ignoring unsupported DHCP message type: mac={mac}, type={other:?}");
        }
    }
}

/// Where a reply to `request` should be sent.
///
/// RFC 2131 §4.1: a client that cannot take delivery of a unicast IP datagram
/// sets the BROADCAST flag, and a server replying directly to a client SHOULD
/// honour it. Deciding purely on whether the request arrived from 0.0.0.0
/// misses that entirely, and the client it strands is one that renews
/// perfectly well: it holds an address, so it asks from that address, and the
/// unicast ACK it cannot accept never registers. It retransmits at the
/// RENEWING floor of 60 seconds until the lease is nearly gone, then releases
/// and starts again from DHCPDISCOVER — which is broadcast, so it succeeds,
/// and the whole cycle repeats. An access point doing that drops its clients
/// every time round.
fn reply_destination(request: &Message, reply: &Message, src_addr: SocketAddr) -> SocketAddr {
    let broadcast = SocketAddr::from(([255, 255, 255, 255], 68));
    // §4.1: a non-zero `giaddr` is the first test, ahead of everything else.
    // The reply goes back to the relay that forwarded it and the relay decides
    // how to deliver it — broadcasting instead would put the reply on this
    // server's own segment, which is not the one the client is on.
    if !request.giaddr().is_unspecified() {
        return SocketAddr::from((request.giaddr(), 67));
    }
    // §4.3.2: a DHCPNAK for a request that arrived without a relay is
    // broadcast. It says the address the client is holding is wrong, so that
    // address is the one place it cannot usefully be sent.
    if reply.opts().msg_type() == Some(MessageType::Nak) {
        return broadcast;
    }
    if request.flags().broadcast() {
        return broadcast;
    }
    // A client still in SELECTING/INIT-REBOOT has no address to be reached at.
    if src_addr.ip().is_unspecified() {
        return broadcast;
    }
    src_addr
}

/// Handle a DHCPDISCOVER message: assign a lease and build an OFFER response.
pub(crate) async fn handle_discover(
    service: &Arc<dyn DhcpService>,
    msg: &Message,
    mac: &str,
) -> Result<Message, AppError> {
    let admin_ctx = AuthContext::system();
    let hostname = extract_hostname(msg);
    let lease = auth_context::with_context(
        admin_ctx.clone(),
        service.assign_lease(mac, hostname.as_deref()),
    )
    .await?;

    // Render the response from the device's resolved per-zone scope (#737),
    // not the global config, so per-zone subnet/gateway/DNS are advertised.
    let scope = auth_context::with_context(admin_ctx, service.scope_for_mac(mac)).await?;

    tracing::info!(
        %mac,
        ip = %lease.ip_address,
        lease_id = %lease.id,
        "sending DHCPOFFER: mac={mac}, ip={ip}",
        mac = mac,
        ip = lease.ip_address,
    );

    Ok(build_response(msg, MessageType::Offer, &lease, &scope))
}

/// Handle a DHCPREQUEST message.
///
/// Delegates to [`DhcpService::evaluate_renewal`], which decides whether the
/// client may keep the IP it asked for. A legitimate renewal produces a
/// DHCPACK; a request for an IP the configuration no longer justifies (e.g.
/// the pool moved out from under it) produces a DHCPNAK, forcing the client
/// back to DISCOVER so it picks up an in-range lease (issue #227).
pub(crate) async fn handle_request(
    service: &Arc<dyn DhcpService>,
    msg: &Message,
    mac: &str,
) -> Result<Message, AppError> {
    let admin_ctx = AuthContext::system();
    let hostname = extract_hostname(msg);
    let requested_ip = extract_requested_ip(msg);

    let lease = auth_context::with_context(
        admin_ctx.clone(),
        service.renew_lease(mac, hostname.as_deref()),
    )
    .await?;

    // Render the response from the device's resolved per-zone scope (#737),
    // not the global config, so per-zone subnet/gateway/DNS are advertised.
    let scope = auth_context::with_context(admin_ctx, service.scope_for_mac(mac)).await?;

    // If the client asked to keep a specific IP but the service assigned a
    // different one, its old address is no longer valid (e.g. the pool moved
    // out from under it). A renewing client expects either an ACK for the same
    // IP or a NAK — silently ACKing a different IP is a protocol violation, so
    // NAK to force it back through DISCOVER, where it picks up the in-range
    // lease that `renew_lease` just prepared. See issue #227.
    if !requested_ip.is_unspecified() && lease.ip_address != requested_ip {
        let new_ip = lease.ip_address;
        tracing::info!(
            %mac,
            %requested_ip,
            %new_ip,
            "sending DHCPNAK: mac={mac} requested out-of-range ip={requested_ip}, assigned new_ip={new_ip}",
        );
        return Ok(build_nak(msg, &scope));
    }

    tracing::info!(
        %mac,
        ip = %lease.ip_address,
        lease_id = %lease.id,
        "sending DHCPACK: mac={mac}, ip={ip}",
        mac = mac,
        ip = lease.ip_address,
    );

    Ok(build_response(msg, MessageType::Ack, &lease, &scope))
}

/// Handle a DHCPRELEASE message.
///
/// A DHCPRELEASE elicits no server response; it just frees the client's lease.
/// The wire `chaddr` is attacker-controllable, so it is **never** treated as
/// proof of lease ownership: any unauthenticated LAN device could otherwise
/// forge a victim's MAC and free the victim's lease, enabling denial of
/// service and IP takeover (CWE-639, finding F3).
///
/// Per RFC 2131 a legitimate DHCPRELEASE is unicast by the client from its own
/// leased address, so we authorize the release on the **network-layer UDP
/// source address** (`src_addr`) — the one field an on-LAN attacker cannot set
/// without genuinely spoofing their IP at the network layer. We require it to
/// match the IP currently recorded for that MAC's active lease, reject an
/// unspecified/broadcast source (which cannot be a legitimate unicast release),
/// and drop the packet without releasing on any mismatch. Only an exact match
/// with the lease's recorded IP runs `release_lease`.
///
/// The wire `ciaddr` is deliberately **not** trusted as the ownership claim:
/// it is attacker-controlled packet payload, decoded straight from the message
/// body exactly as forgeable as `chaddr`. Trusting it would let anyone who
/// knows the victim's IP (trivially, via ARP) forge a release with a plain UDP
/// socket and no spoofing at all, reopening CWE-639. It is only logged, as an
/// informational hint.
pub(crate) async fn handle_release(
    service: &Arc<dyn DhcpService>,
    msg: &Message,
    mac: &str,
    src_addr: SocketAddr,
) {
    let admin_ctx = AuthContext::system();

    // Authorize on the real UDP source address only. `ciaddr` is an untrusted
    // wire field (see this function's doc comment) — kept solely for logging.
    let ciaddr = msg.ciaddr();
    let claimed_ip = match src_addr.ip() {
        IpAddr::V4(v4) => v4,
        // A DHCPv4 client is always reached over IPv4; an IPv6 source cannot
        // own an IPv4 lease, so treat it as unauthenticated.
        IpAddr::V6(_) => Ipv4Addr::UNSPECIFIED,
    };

    // A legitimate unicast release never originates from 0.0.0.0 or the limited
    // broadcast address; such a packet carries no verifiable ownership claim.
    if claimed_ip.is_unspecified() || claimed_ip.is_broadcast() {
        tracing::warn!(
            %mac,
            %src_addr,
            %ciaddr,
            "dropping DHCPRELEASE from unspecified/broadcast source: mac={mac}, src={src_addr}"
        );
        return;
    }

    // Look up the lease currently recorded for this MAC. Only a release whose
    // source matches that lease's own IP is authorised to free it.
    let active = match auth_context::with_context(admin_ctx.clone(), service.active_lease(mac))
        .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(%mac, error = %e, "failed to look up active lease for DHCPRELEASE: {e}");
            return;
        }
    };

    let Some(lease) = active else {
        tracing::debug!(
            %mac,
            %claimed_ip,
            "dropping DHCPRELEASE: no active lease recorded for mac={mac}"
        );
        return;
    };

    if lease.ip_address != claimed_ip {
        tracing::warn!(
            %mac,
            %claimed_ip,
            %ciaddr,
            lease_ip = %lease.ip_address,
            "dropping forged DHCPRELEASE: UDP source does not match the lease's recorded IP (mac={mac}, src={claimed_ip}, lease_ip={lease_ip})",
            lease_ip = lease.ip_address,
        );
        return;
    }

    if let Err(e) = auth_context::with_context(admin_ctx, service.release_lease(mac)).await {
        tracing::error!(%mac, error = %e, "failed to handle DHCPRELEASE for {mac}: {e}");
    }
}

/// Extract the IP address a DHCPREQUEST client wants to keep. In RENEWING /
/// REBINDING the client already holds the address, so it appears in `ciaddr`;
/// in SELECTING / INIT-REBOOT it is carried in the Requested-IP option (50).
/// Returns `0.0.0.0` when neither is present.
fn extract_requested_ip(msg: &Message) -> Ipv4Addr {
    let ciaddr = msg.ciaddr();
    if !ciaddr.is_unspecified() {
        return ciaddr;
    }
    if let Some(DhcpOption::RequestedIpAddress(ip)) = msg.opts().get(OptionCode::RequestedIpAddress)
    {
        return *ip;
    }
    Ipv4Addr::UNSPECIFIED
}

/// Build a DHCPNAK response. A NAK carries no address; it just tells the client
/// its request was rejected and it must restart the handshake. The server
/// identifier is taken from the device's resolved per-zone scope (#737).
pub(crate) fn build_nak(request: &Message, scope: &DhcpScope) -> Message {
    // `request.chaddr()` panics when the wire-supplied hlen exceeds the
    // fixed 16-byte chaddr array; callers must bound it first (issue #829).
    debug_assert!(
        request.hlen() <= 16,
        "build_nak requires a message whose hlen was bounded before dispatch (issue #829)"
    );
    let server_ip = scope.gateway_ip;

    let mut response = Message::default();
    response
        .set_opcode(Opcode::BootReply)
        .set_xid(request.xid())
        .set_flags(Flags::default().set_broadcast())
        .set_chaddr(request.chaddr());

    let opts = response.opts_mut();
    opts.insert(DhcpOption::MessageType(MessageType::Nak));
    opts.insert(DhcpOption::ServerIdentifier(server_ip));

    response
}

/// Build an OFFER or ACK response message.
pub(crate) fn build_response(
    request: &Message,
    msg_type: MessageType,
    lease: &DhcpLease,
    scope: &DhcpScope,
) -> Message {
    // `request.chaddr()` panics when the wire-supplied hlen exceeds the
    // fixed 16-byte chaddr array; callers must bound it first (issue #829).
    debug_assert!(
        request.hlen() <= 16,
        "build_response requires a message whose hlen was bounded before dispatch (issue #829)"
    );
    // Wardnet's IP within this scope: the LAN IP for the base pool, or the Pi's
    // per-zone gateway alias for a zone subnet (#737).
    let server_ip = scope.gateway_ip;

    let mut response = Message::default();
    response
        .set_opcode(Opcode::BootReply)
        .set_xid(request.xid())
        .set_yiaddr(lease.ip_address)
        .set_siaddr(server_ip)
        // RFC 2131 §4.1: the reply carries back the client's BROADCAST bit.
        // Asserting it on a client that did not set it describes a delivery the
        // client never asked for; one that checks the field discards the reply
        // and retransmits at the RENEWING 60-second floor for the life of the
        // lease. Where the datagram actually goes is decided by the caller —
        // `reply_destination` for a REQUEST reply, an unconditional broadcast
        // for an OFFER, whose client has no address to be reached at yet.
        //
        // Only that one bit crosses over. The other 15 are MBZ, and `Flags`
        // round-trips all 16 unmasked, so echoing the field wholesale would
        // reflect whatever an unauthenticated client put in them straight back
        // out — onto the broadcast address, in the case of an OFFER.
        .set_flags(if request.flags().broadcast() {
            Flags::default().set_broadcast()
        } else {
            Flags::default()
        })
        .set_chaddr(request.chaddr());

    let opts = response.opts_mut();
    opts.insert(DhcpOption::MessageType(msg_type));
    opts.insert(DhcpOption::ServerIdentifier(server_ip));
    opts.insert(DhcpOption::AddressLeaseTime(scope.lease_duration_secs));
    opts.insert(DhcpOption::SubnetMask(scope.subnet_mask));

    // Router option: Wardnet gateway first, then optional upstream router for
    // failover (only for the base pool; zone subnets carry no secondary router).
    let mut routers = vec![server_ip];
    if let Some(router_ip) = scope.router_ip
        && router_ip != server_ip
    {
        routers.push(router_ip);
    }
    opts.insert(DhcpOption::Router(routers));

    // DNS servers from the resolved scope. Falls back to advertising the Pi
    // itself when the scope carries no explicit DNS.
    let dns_servers = if scope.dns.is_empty() {
        vec![server_ip]
    } else {
        scope.dns.clone()
    };
    opts.insert(DhcpOption::DomainNameServer(dns_servers));

    // Member-isolation zones advertise a /32 mask (option 1) so peers appear
    // off-link and every packet is forced through the Pi (#737). A /32 leaves a
    // strict client with no on-link route at all — not even to its own gateway —
    // so we also advertise option 121 (Classless Static Route) with a default
    // route via the gateway. Cooperating clients then install a route to the Pi
    // and reach off-link/upstream destinations through it. RFC 3442 says a
    // client honouring option 121 SHOULD ignore the Router option (3); we still
    // send option 3 for clients that ignore 121.
    if scope.member_isolation
        && let Ok(default_route) = "0.0.0.0/0".parse::<ipnet::Ipv4Net>()
    {
        opts.insert(DhcpOption::ClasslessStaticRoute(vec![(
            default_route,
            server_ip,
        )]));
    }

    response
}

/// Encode and send a DHCP response message to the client.
///
/// In production, real DHCP clients send from `0.0.0.0:68` via broadcast and
/// the response travels back the same path. Sending to `dest` (the address we
/// received the packet from) works correctly in both production and loopback
/// test scenarios.
pub(crate) async fn send_response(socket: &dyn DhcpSocket, msg: &Message, dest: SocketAddr) {
    let mut buf = Vec::with_capacity(512);
    let mut encoder = Encoder::new(&mut buf);

    if let Err(e) = msg.encode(&mut encoder) {
        tracing::error!(error = %e, "failed to encode DHCP response: {e}");
        return;
    }

    if let Err(e) = socket.send_to(&buf, dest).await {
        tracing::error!(error = %e, dest = %dest, "failed to send DHCP response to {dest}: {e}");
    }
}

/// Decode a DHCP message from untrusted wire bytes, rejecting any message
/// whose wire-supplied `hlen` overruns the fixed 16-byte BOOTP `chaddr`
/// field. `dhcproto` slices its `[u8; 16]` chaddr array by `hlen`, so
/// calling `chaddr()` on such a message panics (issue #829). Every decode
/// of raw wire bytes must go through this bound (or a stricter guard, like
/// `server_loop`'s Ethernet-only check) before the message reaches code
/// that may touch `chaddr()`.
pub(crate) fn decode_bounded(packet: &[u8]) -> Option<Message> {
    Message::decode(&mut Decoder::new(packet))
        .ok()
        .filter(|msg| msg.hlen() <= 16)
}

/// Format the first 6 bytes of a hardware address as a MAC string.
pub(crate) fn format_mac(chaddr: &[u8]) -> String {
    let bytes = if chaddr.len() >= 6 {
        &chaddr[..6]
    } else {
        chaddr
    };

    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Extract the hostname from DHCP option 12 if present.
pub(crate) fn extract_hostname(msg: &Message) -> Option<String> {
    for (_code, opt) in msg.opts().iter() {
        if let DhcpOption::Hostname(h) = opt {
            return Some(h.clone());
        }
    }
    None
}

/// Record whatever identification signals this DHCP message carries.
///
/// Best-effort by design: a failure here is logged and swallowed, never
/// propagated. A device must still get its lease if we cannot write an
/// observability row (issue #1099).
///
/// Deliberately spawned rather than awaited on the packet path. Recording costs
/// a MAC lookup plus up to three upserts against the *writer* pool; awaiting it
/// would put an observability side-effect in front of every OFFER and ACK, so a
/// slow writer (a blocklist bulk import, say) would delay leases. DHCP clients
/// retry, but a lease that arrives late for a reason unrelated to leasing is
/// exactly the coupling to avoid.
///
/// Runs under an explicit admin [`AuthContext`] because the identification
/// service is auth-gated like every other service, and the DHCP packet loop is
/// a background task with no ambient context of its own.
/// Returns the spawned task's handle so tests can await it deterministically.
/// The packet loop ignores it — that is the whole point of spawning.
pub(crate) fn record_dhcp_signals(
    identification: Option<&Arc<dyn DeviceIdentificationService>>,
    msg: &Message,
    mac: &str,
) -> Option<tokio::task::JoinHandle<()>> {
    let identification = identification?;

    // Extract on the packet path (the message does not outlive this call), then
    // hand the owned values to the spawned task.
    let signals: Vec<(DeviceSignalKind, String)> = [
        (DeviceSignalKind::DhcpHostname, extract_hostname(msg)),
        (DeviceSignalKind::DhcpParamList, extract_param_list(msg)),
        (DeviceSignalKind::DhcpVendorClass, extract_vendor_class(msg)),
    ]
    .into_iter()
    .filter_map(|(kind, value)| value.map(|v| (kind, v)))
    .collect();

    if signals.is_empty() {
        return None;
    }

    let identification = Arc::clone(identification);
    let mac = mac.to_owned();
    Some(tokio::spawn(async move {
        for (kind, value) in signals {
            let result = auth_context::with_context(
                AuthContext::system(),
                identification.record_signal_for_mac(&mac, kind, &value),
            )
            .await;
            if let Err(error) = result {
                tracing::debug!(%mac, ?kind, %error, "failed to record DHCP identification signal");
            }
        }
    }))
}

/// Extract DHCP option 55 (the parameter request list) as a comma-separated
/// list of option codes, preserving the order the client sent them in.
///
/// The *ordering* is the identifying part (issue #1099): DHCP clients emit a
/// stable, implementation-specific sequence, which is what makes option 55 a
/// device-class fingerprint. Sorting or de-duplicating would destroy exactly
/// the signal we are capturing, so the codes are serialised verbatim.
pub(crate) fn extract_param_list(msg: &Message) -> Option<String> {
    for (_code, opt) in msg.opts().iter() {
        if let DhcpOption::ParameterRequestList(codes) = opt {
            if codes.is_empty() {
                return None;
            }
            return Some(
                codes
                    .iter()
                    .map(|c| u8::from(*c).to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
    }
    None
}

/// Extract DHCP option 60 (vendor class identifier) if present.
///
/// Many `IoT` stacks put a literal vendor or product string here, which the
/// curated vendor catalog can match on.
pub(crate) fn extract_vendor_class(msg: &Message) -> Option<String> {
    for (_code, opt) in msg.opts().iter() {
        if let DhcpOption::ClassIdentifier(raw) = opt {
            // Option 60 is a byte string, not guaranteed UTF-8. Render it
            // lossily rather than dropping the signal: a mangled character is
            // still a usable substring match, whereas discarding the option
            // loses the only vendor tell some devices ever emit.
            let text = String::from_utf8_lossy(raw).trim().to_owned();
            if text.is_empty() {
                return None;
            }
            return Some(text);
        }
    }
    None
}
