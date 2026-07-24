use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use dhcproto::v4::{DhcpOption, Flags, HType, Message, MessageType, Opcode, OptionCode};
use dhcproto::{Decodable, Decoder, Encodable, Encoder};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::dhcp::{DhcpLease, DhcpScope};
use wardnetd_services::auth_context;
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
            server_loop(socket, service, running.clone(), cancel, own_macs).await;
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
// Server loop and helpers
// ---------------------------------------------------------------------------

/// Main server loop: receive DHCP packets, decode, dispatch, and respond.
pub(crate) async fn server_loop(
    socket: Arc<dyn DhcpSocket>,
    service: Arc<dyn DhcpService>,
    running: Arc<AtomicBool>,
    cancel: CancellationToken,
    own_macs: Arc<HashSet<String>>,
) {
    let mut buf = vec![0u8; 1500];

    loop {
        let (len, src_addr) = tokio::select! {
            () = cancel.cancelled() => break,
            result = socket.recv_from(&mut buf) => {
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

        match msg_type {
            MessageType::Discover => match handle_discover(&service, &msg, &mac).await {
                Ok(response) => {
                    // DHCP OFFERs must be broadcast — the client has no IP yet
                    // and can only receive broadcast packets.
                    let broadcast = SocketAddr::from(([255, 255, 255, 255], 68));
                    send_response(socket.as_ref(), &response, broadcast).await;
                }
                Err(e) => {
                    tracing::error!(%mac, error = %e, "failed to handle DHCPDISCOVER for {mac}: {e}");
                }
            },
            MessageType::Request => match handle_request(&service, &msg, &mac).await {
                Ok(response) => {
                    // If the client is requesting from 0.0.0.0 (new lease), send
                    // via broadcast. Renewals come from the client's existing IP
                    // and can be unicast.
                    let dest = if src_addr.ip().is_unspecified() {
                        SocketAddr::from(([255, 255, 255, 255], 68))
                    } else {
                        src_addr
                    };
                    send_response(socket.as_ref(), &response, dest).await;
                }
                Err(e) => {
                    tracing::error!(%mac, error = %e, "failed to handle DHCPREQUEST for {mac}: {e}");
                }
            },
            MessageType::Release => {
                handle_release(&service, &msg, &mac, src_addr).await;
            }
            other => {
                tracing::debug!(%mac, ?other, "ignoring unsupported DHCP message type: mac={mac}, type={other:?}");
            }
        }
    }

    running.store(false, Ordering::SeqCst);
}

/// Handle a DHCPDISCOVER message: assign a lease and build an OFFER response.
pub(crate) async fn handle_discover(
    service: &Arc<dyn DhcpService>,
    msg: &Message,
    mac: &str,
) -> Result<Message, AppError> {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
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
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
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
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

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
        .set_flags(Flags::default().set_broadcast())
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
