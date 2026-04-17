use async_trait::async_trait;
use wardnet_common::tunnel::{Tunnel, TunnelConfig};

/// Insert-only data transfer object for creating a new tunnel record.
///
/// All fields are strings matching the database columns. The service layer
/// constructs this from parsed `WireGuard` config and user input.
#[derive(Debug, Clone)]
pub struct TunnelRow {
    /// UUID primary key.
    pub id: String,
    /// Human-readable label chosen by the user.
    pub label: String,
    /// ISO 3166-1 alpha-2 country code for the exit node.
    pub country_code: String,
    /// Optional VPN provider name.
    pub provider: Option<String>,
    /// `WireGuard` interface name (e.g. `wg_ward0`).
    pub interface_name: String,
    /// Peer endpoint in `host:port` form.
    pub endpoint: String,
    /// Tunnel status string (`"up"`, `"down"`, `"connecting"`).
    pub status: String,
    /// JSON-encoded list of local addresses.
    pub address: String,
    /// JSON-encoded list of DNS servers.
    pub dns: String,
    /// JSON-encoded peer configuration (public key, allowed IPs, etc.).
    pub peer_config: String,
    /// Optional listen port for the local interface.
    pub listen_port: Option<u16>,
}

/// Data access for `WireGuard` tunnel records.
///
/// Provides CRUD operations plus helpers for interface naming and
/// live-stats updates. All business logic (e.g. bring-up/tear-down
/// orchestration) belongs in [`TunnelService`](crate::service::TunnelService).
#[async_trait]
pub trait TunnelRepository: Send + Sync {
    /// Return all tunnels.
    async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>>;

    /// Find a tunnel by primary key.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Tunnel>>;

    /// Retrieve the stored `WireGuard` configuration for a tunnel.
    ///
    /// Returns the address list, DNS servers, listen port, and peer config
    /// needed to build [`CreateTunnelParams`](crate::tunnel_interface::CreateTunnelParams)
    /// when bringing a tunnel up.
    async fn find_config_by_id(&self, id: &str) -> anyhow::Result<Option<TunnelConfig>>;

    /// Insert a new tunnel record.
    async fn insert(&self, row: &TunnelRow) -> anyhow::Result<()>;

    /// Update the status of a tunnel.
    async fn update_status(&self, id: &str, status: &str) -> anyhow::Result<()>;

    /// Update live stats for a tunnel.
    async fn update_stats(
        &self,
        id: &str,
        bytes_tx: i64,
        bytes_rx: i64,
        last_handshake: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Delete a tunnel by ID.
    async fn delete(&self, id: &str) -> anyhow::Result<()>;

    /// Return the next available interface index (for `wg_ward{N}` naming).
    async fn next_interface_index(&self) -> anyhow::Result<i64>;

    /// Return the total number of tunnels.
    async fn count(&self) -> anyhow::Result<i64>;

    /// Count tunnels with status `"up"`.
    async fn count_active(&self) -> anyhow::Result<i64>;
}
