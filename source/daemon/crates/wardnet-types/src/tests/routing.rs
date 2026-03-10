use crate::routing::*;
use uuid::Uuid;

#[test]
fn routing_target_tunnel_tagged() {
    let target = RoutingTarget::Tunnel {
        tunnel_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
    };
    let json = serde_json::to_string(&target).unwrap();
    assert!(json.contains("\"type\":\"tunnel\""));
    assert!(json.contains("\"tunnel_id\""));
    let back: RoutingTarget = serde_json::from_str(&json).unwrap();
    assert_eq!(target, back);
}

#[test]
fn routing_target_direct_tagged() {
    let json = serde_json::to_string(&RoutingTarget::Direct).unwrap();
    assert_eq!(json, "{\"type\":\"direct\"}");
    let back: RoutingTarget = serde_json::from_str(&json).unwrap();
    assert_eq!(back, RoutingTarget::Direct);
}

#[test]
fn routing_target_default_tagged() {
    let json = serde_json::to_string(&RoutingTarget::Default).unwrap();
    assert_eq!(json, "{\"type\":\"default\"}");
    let back: RoutingTarget = serde_json::from_str(&json).unwrap();
    assert_eq!(back, RoutingTarget::Default);
}

#[test]
fn rule_creator_round_trip() {
    for creator in [RuleCreator::Admin, RuleCreator::User] {
        let json = serde_json::to_string(&creator).unwrap();
        let back: RuleCreator = serde_json::from_str(&json).unwrap();
        assert_eq!(creator, back);
    }
}

#[test]
fn routing_rule_round_trip() {
    let rule = RoutingRule {
        device_id: Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap(),
        target: RoutingTarget::Direct,
        created_by: RuleCreator::User,
    };
    let json = serde_json::to_string(&rule).unwrap();
    let back: RoutingRule = serde_json::from_str(&json).unwrap();
    assert_eq!(rule.device_id, back.device_id);
    assert_eq!(rule.target, back.target);
    assert_eq!(rule.created_by, back.created_by);
}
