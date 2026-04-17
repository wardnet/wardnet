use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The current status of a `WireGuard` tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelStatus {
    Up,
    Down,
    Connecting,
}

/// A `WireGuard` tunnel configuration and its live state.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    pub address: Vec<String>,
    pub dns: Vec<String>,
    pub listen_port: Option<u16>,
    pub peer: WgPeerConfig,
}
