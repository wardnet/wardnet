use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The current status of a `WireGuard` tunnel.
///
/// State machine driven by user/admin actions and the tunnel monitor's
/// health-check loop:
///
/// - `Down` — kernel interface not configured (initial state, after
///   tear-down, or after delete).
/// - `Connecting` — `bring_up` succeeded; kernel interface is configured;
///   no handshake observed yet.
/// - `Up` — recent (≤ 3 min) handshake observed.
/// - `Reconnecting` — was `Up`, last handshake stale (> 3 min) or absent;
///   the iface is still configured and `WireGuard` keepalive is retrying.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TunnelStatus {
    Up,
    Down,
    Connecting,
    Reconnecting,
}

/// A `WireGuard` tunnel configuration and its live state.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Tunnel {
    pub id: Uuid,
    pub label: String,
    pub country_code: String,
    pub provider: Option<String>,
    pub interface_name: String,
    pub endpoint: String,
    pub status: TunnelStatus,
    pub last_handshake: Option<DateTime<Utc>>,
    pub bytes_tx: u64,
    pub bytes_rx: u64,
    pub created_at: DateTime<Utc>,
}

use crate::wireguard_config::WgPeerConfig;

/// Persisted tunnel configuration (excludes private key).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelConfig {
    pub address: Vec<String>,
    pub dns: Vec<String>,
    pub listen_port: Option<u16>,
    pub peer: WgPeerConfig,
}
