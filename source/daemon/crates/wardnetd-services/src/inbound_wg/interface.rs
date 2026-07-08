use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ipnetwork::IpNetwork;

/// Configuration for the inbound `WireGuard` **server** interface (issue #809).
///
/// Unlike the outbound single-peer [`TunnelConfig`](crate::tunnel::TunnelConfig),
/// this describes only the server side: its own keypair, listen port, and the
/// address(es) it owns on the inbound tunnel subnet. Peers are added separately
/// via [`InboundWgInterface::add_peer`] so the peer list can grow and shrink
/// without recreating the interface.
#[derive(Debug, Clone)]
pub struct InboundWgServerConfig {
    /// Name of the server interface (e.g. `wg_wardin0`).
    pub interface_name: String,
    /// The server's raw 32-byte private key.
    pub private_key: [u8; 32],
    /// UDP port the server listens on for inbound handshakes.
    pub listen_port: u16,
    /// Address(es) assigned to the server interface (e.g. `10.100.64.1/24`).
    pub address: Vec<IpNetwork>,
}

/// Configuration for a single inbound peer admitted onto the server (issue #809).
#[derive(Debug, Clone)]
pub struct InboundWgPeerConfig {
    /// The peer's raw 32-byte public key.
    pub public_key: [u8; 32],
    /// CIDRs routed to this peer — its fixed `/32` on the inbound subnet.
    pub allowed_ips: Vec<IpNetwork>,
    /// Optional raw 32-byte preshared key.
    pub preshared_key: Option<[u8; 32]>,
    /// Keepalive interval in seconds, if desired.
    pub persistent_keepalive: Option<u16>,
}

/// Per-peer live stats read from the server interface (issue #809).
///
/// Unlike the outbound tunnel — which aggregates all peers into one
/// [`TunnelStats`](crate::tunnel::TunnelStats) — an inbound server keeps stats
/// per peer, since an admin wants to see each peer's own activity.
#[derive(Debug, Clone)]
pub struct InboundWgPeerStats {
    /// The peer's raw 32-byte public key (identifies which peer this is).
    pub public_key: [u8; 32],
    /// Bytes transmitted to this peer.
    pub bytes_tx: u64,
    /// Bytes received from this peer.
    pub bytes_rx: u64,
    /// Most recent handshake time with this peer.
    pub last_handshake: Option<DateTime<Utc>>,
}

/// Abstraction over the inbound `WireGuard` **server** interface (issue #809).
///
/// Peer-list-shaped: the server is stood up once, then peers are added and
/// removed incrementally without disturbing the interface or the other peers.
/// Enables mocking in tests. The production implementation uses the
/// `wireguard-control` crate + `ip` CLI on Linux.
#[async_trait]
pub trait InboundWgInterface: Send + Sync {
    /// Create (or reconfigure) the server interface with the given config.
    ///
    /// Idempotent: safe to call on every enable/startup. Existing peers on the
    /// interface are preserved — only the server-level config (key, port,
    /// address) is (re)applied.
    async fn ensure_server(&self, config: InboundWgServerConfig) -> anyhow::Result<()>;

    /// Tear the server interface down entirely, dropping all peers.
    async fn tear_down_server(&self, interface_name: &str) -> anyhow::Result<()>;

    /// Add a single peer to the server. Incremental — never touches existing
    /// peers.
    async fn add_peer(&self, interface_name: &str, peer: InboundWgPeerConfig)
    -> anyhow::Result<()>;

    /// Remove a single peer from the server by its public key.
    async fn remove_peer(&self, interface_name: &str, public_key: [u8; 32]) -> anyhow::Result<()>;

    /// Read per-peer stats for every peer currently on the server interface.
    async fn peer_stats(&self, interface_name: &str) -> anyhow::Result<Vec<InboundWgPeerStats>>;
}
