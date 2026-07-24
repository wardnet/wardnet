use wardnet_common::routing::RoutingTarget;

use crate::commands::{parse_routing_target, parse_uuid};
use crate::error::CliError;

#[test]
fn routing_target_keywords() {
    assert_eq!(
        parse_routing_target("direct").unwrap(),
        RoutingTarget::Direct
    );
    assert_eq!(
        parse_routing_target("default").unwrap(),
        RoutingTarget::Default
    );
}

#[test]
fn routing_target_tunnel_uuid() {
    let id = "550e8400-e29b-41d4-a716-446655440000";
    assert_eq!(
        parse_routing_target(id).unwrap(),
        RoutingTarget::Tunnel {
            tunnel_id: id.parse().unwrap()
        }
    );
}

#[test]
fn routing_target_rejects_garbage() {
    assert!(matches!(
        parse_routing_target("nope"),
        Err(CliError::Usage(_))
    ));
}

#[test]
fn uuid_parse_rejects_non_uuid() {
    assert!(matches!(
        parse_uuid("device", "abc"),
        Err(CliError::Usage(_))
    ));
}
