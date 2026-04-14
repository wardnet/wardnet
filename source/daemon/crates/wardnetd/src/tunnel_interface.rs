use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ipnetwork::IpNetwork;
use std::net::SocketAddr;

/// Configuration for creating a tunnel interface.
#[derive(Debug, Clone)]
pub struct CreateTunnelParams {
    /// Name of the tunnel interface (e.g. `wg_ward0`).
    pub interface_name: String,
    /// Technology-specific tunnel configuration.
    pub config: TunnelConfig,
}

/// Technology-specific tunnel configuration.
#[derive(Debug, Clone)]
pub enum TunnelConfig {
    /// `WireGuard` tunnel configuration.
    WireGuard {
        /// Interface addresses from the `[Interface] Address` field (e.g. `10.99.1.2/24`).
        address: Vec<IpNetwork>,
        /// Raw 32-byte private key.
        private_key: [u8; 32],
        /// Optional listen port for incoming connections.
        listen_port: Option<u16>,
        /// Raw 32-byte peer public key.
        peer_public_key: [u8; 32],
        /// Remote endpoint of the peer.
        peer_endpoint: Option<SocketAddr>,
        /// CIDRs the peer is allowed to route.
        peer_allowed_ips: Vec<IpNetwork>,
        /// Optional raw 32-byte preshared key.
        peer_preshared_key: Option<[u8; 32]>,
        /// Keepalive interval in seconds, if desired.
        persistent_keepalive: Option<u16>,
    },
}

/// Live stats from a tunnel interface.
#[derive(Debug, Clone)]
pub struct TunnelStats {
    /// Total bytes transmitted across all peers.
    pub bytes_tx: u64,
    /// Total bytes received across all peers.
    pub bytes_rx: u64,
    /// Most recent handshake time among all peers.
    pub last_handshake: Option<DateTime<Utc>>,
}

/// Abstraction over tunnel interface operations (create, up, down, remove, stats).
///
/// Enables mocking in tests. The production implementation uses the
/// `wireguard-control` crate for `WireGuard` tunnels.
#[async_trait]
pub trait TunnelInterface: Send + Sync {
    /// Create a tunnel interface with the given configuration.
    async fn create(&self, params: CreateTunnelParams) -> anyhow::Result<()>;

    /// Bring a tunnel interface up.
    async fn bring_up(&self, interface_name: &str) -> anyhow::Result<()>;

    /// Tear down a tunnel interface.
    async fn tear_down(&self, interface_name: &str) -> anyhow::Result<()>;

    /// Remove a tunnel interface entirely.
    async fn remove(&self, interface_name: &str) -> anyhow::Result<()>;

    /// Query the current stats for a tunnel interface.
    async fn get_stats(&self, interface_name: &str) -> anyhow::Result<Option<TunnelStats>>;

    /// List all existing tunnel interface names.
    async fn list(&self) -> anyhow::Result<Vec<String>>;
}
