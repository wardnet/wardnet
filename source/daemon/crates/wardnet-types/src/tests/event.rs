use crate::event::*;
use crate::routing::{RoutingTarget, RuleCreator};
use crate::tunnel::TunnelStatus;
use uuid::Uuid;

#[test]
fn device_discovered_tagged() {
    let event = WardnetEvent::DeviceDiscovered {
        device_id: Uuid::nil(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        ip: "192.168.1.10".to_owned(),
        hostname: Some("myphone".to_owned()),
        timestamp: "2026-03-07T00:00:00Z".parse().unwrap(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"device_discovered\""));
    let back: WardnetEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, WardnetEvent::DeviceDiscovered { .. }));
}

#[test]
fn tunnel_stats_updated_round_trip() {
    let event = WardnetEvent::TunnelStatsUpdated {
        tunnel_id: Uuid::nil(),
        status: TunnelStatus::Up,
        bytes_tx: 1000,
        bytes_rx: 2000,
        last_handshake: None,
        timestamp: "2026-03-07T00:00:00Z".parse().unwrap(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: WardnetEvent = serde_json::from_str(&json).unwrap();
    if let WardnetEvent::TunnelStatsUpdated {
        bytes_tx, bytes_rx, ..
    } = back
    {
        assert_eq!(bytes_tx, 1000);
        assert_eq!(bytes_rx, 2000);
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn routing_rule_changed_round_trip() {
    let event = WardnetEvent::RoutingRuleChanged {
        device_id: Uuid::nil(),
        target: RoutingTarget::Direct,
        previous_target: Some(RoutingTarget::Default),
        changed_by: RuleCreator::Admin,
        timestamp: "2026-03-07T00:00:00Z".parse().unwrap(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"routing_rule_changed\""));
    let back: WardnetEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, WardnetEvent::RoutingRuleChanged { .. }));
}
