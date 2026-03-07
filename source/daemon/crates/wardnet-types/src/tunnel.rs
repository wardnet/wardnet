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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunnel_status_round_trip() {
        for status in [
            TunnelStatus::Up,
            TunnelStatus::Down,
            TunnelStatus::Connecting,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: TunnelStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    #[test]
    fn tunnel_status_rename_snake_case() {
        assert_eq!(
            serde_json::to_string(&TunnelStatus::Connecting).unwrap(),
            "\"connecting\""
        );
    }

    #[test]
    fn tunnel_round_trip() {
        let tunnel = Tunnel {
            id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            label: "US East".to_owned(),
            country_code: "US".to_owned(),
            provider: Some("Mullvad".to_owned()),
            interface_name: "wg0".to_owned(),
            endpoint: "1.2.3.4:51820".to_owned(),
            status: TunnelStatus::Up,
            last_handshake: Some("2026-03-07T00:00:00Z".parse().unwrap()),
            bytes_tx: 1024,
            bytes_rx: 2048,
        };
        let json = serde_json::to_string(&tunnel).unwrap();
        let back: Tunnel = serde_json::from_str(&json).unwrap();
        assert_eq!(tunnel.id, back.id);
        assert_eq!(tunnel.label, back.label);
        assert_eq!(tunnel.status, back.status);
        assert_eq!(tunnel.bytes_tx, back.bytes_tx);
    }
}
