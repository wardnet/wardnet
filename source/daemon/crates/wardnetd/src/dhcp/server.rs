use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use dhcproto::v4::{DhcpOption, Flags, Message, MessageType, Opcode};
use dhcproto::{Decodable, Decoder, Encodable, Encoder};
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wardnet_types::auth::AuthContext;
use wardnet_types::dhcp::{DhcpConfig, DhcpLease};

use crate::auth_context;
use crate::error::AppError;
use crate::service::DhcpService;

// ---------------------------------------------------------------------------
// DhcpSocket trait
// ---------------------------------------------------------------------------

/// Abstraction over UDP socket operations for DHCP packet I/O.
///
/// Allows injecting a mock socket in tests instead of binding a real
/// UDP port.
#[async_trait]
pub trait DhcpSocket: Send + Sync {
    /// Receive a DHCP packet, returning the number of bytes read and
    /// the source address.
    async fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)>;

    /// Send a DHCP response to the given destination.
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> std::io::Result<usize>;
}

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
// DhcpServer trait
// ---------------------------------------------------------------------------

/// Abstraction over the raw DHCP packet handling server.
///
/// Allows testing with a noop or mock implementation.
#[async_trait]
pub trait DhcpServer: Send + Sync {
    /// Start listening for DHCP packets on UDP port 67.
    async fn start(&self) -> Result<(), AppError>;

    /// Stop the running server.
    async fn stop(&self) -> Result<(), AppError>;

    /// Whether the server is currently running.
    fn is_running(&self) -> bool;
}

// ---------------------------------------------------------------------------
// UdpDhcpServer
// ---------------------------------------------------------------------------

/// Production DHCP server that processes DISCOVER/REQUEST/RELEASE
/// messages using the service layer.
pub struct UdpDhcpServer {
    /// Service for lease management.
    service: Arc<dyn DhcpService>,
    /// Current DHCP configuration (updated via `RwLock`).
    config: Arc<RwLock<DhcpConfig>>,
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
    pub fn new(service: Arc<dyn DhcpService>, config: DhcpConfig) -> Self {
        Self::with_bind_addr(service, config, SocketAddr::from(([0, 0, 0, 0], 67)))
    }

    /// Create a new DHCP server that binds to the given address.
    ///
    /// Use `127.0.0.1:0` in tests so the OS assigns an ephemeral port and
    /// the server operates entirely over loopback.
    #[must_use]
    pub(crate) fn with_bind_addr(
        service: Arc<dyn DhcpService>,
        config: DhcpConfig,
        bind_addr: SocketAddr,
    ) -> Self {
        Self {
            service,
            config: Arc::new(RwLock::new(config)),
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
    pub(crate) fn with_socket(
        service: Arc<dyn DhcpService>,
        config: DhcpConfig,
        socket: Arc<dyn DhcpSocket>,
    ) -> Self {
        Self {
            service,
            config: Arc::new(RwLock::new(config)),
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

    /// Update the stored configuration (called when config changes).
    pub async fn update_config(&self, config: DhcpConfig) {
        *self.config.write().await = config;
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

            tracing::info!(%actual_addr, "DHCP server listening");

            Arc::new(udp_socket)
        };

        let service = Arc::clone(&self.service);
        let config = Arc::clone(&self.config);
        let running = Arc::clone(&self.running);

        // Create a fresh cancellation token so stop()/start() cycles work.
        let new_cancel = CancellationToken::new();
        let cancel = new_cancel.clone();
        *self.cancel.lock().await = new_cancel;

        running.store(true, Ordering::SeqCst);

        let handle = tokio::spawn(async move {
            server_loop(socket, service, config, running.clone(), cancel).await;
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
    config: Arc<RwLock<DhcpConfig>>,
    running: Arc<AtomicBool>,
    cancel: CancellationToken,
) {
    let mut buf = vec![0u8; 1500];

    loop {
        let (len, src_addr) = tokio::select! {
            () = cancel.cancelled() => break,
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!(error = %e, "DHCP socket recv error");
                        continue;
                    }
                }
            }
        };

        let packet = &buf[..len];
        let msg = match Message::decode(&mut Decoder::new(packet)) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(error = %e, "failed to decode DHCP message");
                continue;
            }
        };

        let Some(msg_type) = msg.opts().msg_type() else {
            tracing::debug!("DHCP message has no message type option, ignoring");
            continue;
        };

        let mac = format_mac(msg.chaddr());
        tracing::debug!(%mac, ?msg_type, xid = msg.xid(), "received DHCP message");

        let cfg = config.read().await.clone();

        match msg_type {
            MessageType::Discover => match handle_discover(&service, &msg, &mac, &cfg).await {
                Ok(response) => {
                    send_response(socket.as_ref(), &response, src_addr).await;
                }
                Err(e) => {
                    tracing::error!(%mac, error = %e, "failed to handle DHCPDISCOVER");
                }
            },
            MessageType::Request => match handle_request(&service, &msg, &mac, &cfg).await {
                Ok(response) => {
                    send_response(socket.as_ref(), &response, src_addr).await;
                }
                Err(e) => {
                    tracing::error!(%mac, error = %e, "failed to handle DHCPREQUEST");
                }
            },
            MessageType::Release => {
                let admin_ctx = AuthContext::Admin {
                    admin_id: Uuid::nil(),
                };
                if let Err(e) =
                    auth_context::with_context(admin_ctx, service.release_lease(&mac)).await
                {
                    tracing::error!(%mac, error = %e, "failed to handle DHCPRELEASE");
                }
            }
            other => {
                tracing::debug!(%mac, ?other, "ignoring unsupported DHCP message type");
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
    config: &DhcpConfig,
) -> Result<Message, AppError> {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    let hostname = extract_hostname(msg);
    let lease =
        auth_context::with_context(admin_ctx, service.assign_lease(mac, hostname.as_deref()))
            .await?;

    tracing::info!(
        %mac,
        ip = %lease.ip_address,
        lease_id = %lease.id,
        "sending DHCPOFFER"
    );

    Ok(build_response(msg, MessageType::Offer, &lease, config))
}

/// Handle a DHCPREQUEST message: renew the lease and build an ACK response.
pub(crate) async fn handle_request(
    service: &Arc<dyn DhcpService>,
    msg: &Message,
    mac: &str,
    config: &DhcpConfig,
) -> Result<Message, AppError> {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    let lease = auth_context::with_context(admin_ctx, service.renew_lease(mac)).await?;

    tracing::info!(
        %mac,
        ip = %lease.ip_address,
        lease_id = %lease.id,
        "sending DHCPACK"
    );

    Ok(build_response(msg, MessageType::Ack, &lease, config))
}

/// Build an OFFER or ACK response message.
pub(crate) fn build_response(
    request: &Message,
    msg_type: MessageType,
    lease: &DhcpLease,
    config: &DhcpConfig,
) -> Message {
    // Use the server's own IP. For DHCP on LAN, this is typically the
    // Wardnet gateway IP. We use the pool start's network as a heuristic,
    // or the router_ip if configured. A proper implementation would detect
    // the interface IP, but for MVP we use a sensible default.
    let server_ip = config.router_ip.unwrap_or(config.pool_start);

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
    opts.insert(DhcpOption::AddressLeaseTime(config.lease_duration_secs));
    opts.insert(DhcpOption::SubnetMask(config.subnet_mask));

    // Router option: Wardnet gateway first, then upstream router for failover.
    let mut routers = vec![server_ip];
    if let Some(router_ip) = config.router_ip
        && router_ip != server_ip
    {
        routers.push(router_ip);
    }
    opts.insert(DhcpOption::Router(routers));

    // Wardnet IS the DNS server -- advertise itself.
    opts.insert(DhcpOption::DomainNameServer(vec![server_ip]));

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
        tracing::error!(error = %e, "failed to encode DHCP response");
        return;
    }

    if let Err(e) = socket.send_to(&buf, dest).await {
        tracing::error!(error = %e, dest = %dest, "failed to send DHCP response");
    }
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

/// No-op DHCP server implementation for testing and `--mock-network` mode.
pub struct NoopDhcpServer;

#[async_trait]
impl DhcpServer for NoopDhcpServer {
    async fn start(&self) -> Result<(), AppError> {
        tracing::info!("noop DHCP server: start (no-op)");
        Ok(())
    }

    async fn stop(&self) -> Result<(), AppError> {
        tracing::info!("noop DHCP server: stop (no-op)");
        Ok(())
    }

    fn is_running(&self) -> bool {
        false
    }
}
