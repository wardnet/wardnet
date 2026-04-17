use crate::tunnel::{Tunnel, TunnelStatus};
use uuid::Uuid;

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
        created_at: "2026-03-07T00:00:00Z".parse().unwrap(),
    };
    let json = serde_json::to_string(&tunnel).unwrap();
    let back: Tunnel = serde_json::from_str(&json).unwrap();
    assert_eq!(tunnel.id, back.id);
    assert_eq!(tunnel.label, back.label);
    assert_eq!(tunnel.status, back.status);
    assert_eq!(tunnel.bytes_tx, back.bytes_tx);
    assert_eq!(tunnel.created_at, back.created_at);
}
