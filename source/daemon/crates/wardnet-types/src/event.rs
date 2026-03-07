use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::routing::{RoutingTarget, RuleCreator};
use crate::tunnel::TunnelStatus;

/// Domain events emitted by the Wardnet daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WardnetEvent {
    DeviceDiscovered {
        device_id: Uuid,
        mac: String,
        ip: String,
        hostname: Option<String>,
        timestamp: DateTime<Utc>,
    },
    DeviceIpChanged {
        device_id: Uuid,
        mac: String,
        old_ip: String,
        new_ip: String,
        timestamp: DateTime<Utc>,
    },
    DeviceGone {
        device_id: Uuid,
        mac: String,
        last_ip: String,
        timestamp: DateTime<Utc>,
    },
    TunnelUp {
        tunnel_id: Uuid,
        interface_name: String,
        endpoint: String,
        timestamp: DateTime<Utc>,
    },
    TunnelDown {
        tunnel_id: Uuid,
        interface_name: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    TunnelStatsUpdated {
        tunnel_id: Uuid,
        status: TunnelStatus,
        bytes_tx: u64,
        bytes_rx: u64,
        last_handshake: Option<DateTime<Utc>>,
        timestamp: DateTime<Utc>,
    },
    RoutingRuleChanged {
        device_id: Uuid,
        target: RoutingTarget,
        previous_target: Option<RoutingTarget>,
        changed_by: RuleCreator,
        timestamp: DateTime<Utc>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
