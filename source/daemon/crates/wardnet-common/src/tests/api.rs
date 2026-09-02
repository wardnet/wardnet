use crate::api::{ApiError, ExportBackupRequest, LoginRequest, SetMyRuleRequest, SetupRequest};
use crate::routing::RoutingTarget;

#[test]
fn set_my_rule_request_deserializes_tunnel() {
    let json = r#"{"target":{"type":"tunnel","tunnel_id":"00000000-0000-0000-0000-000000000001"}}"#;
    let req: SetMyRuleRequest = serde_json::from_str(json).unwrap();
    assert!(matches!(req.target, RoutingTarget::Tunnel { .. }));
}

#[test]
fn set_my_rule_request_deserializes_direct() {
    let json = r#"{"target":{"type":"direct"}}"#;
    let req: SetMyRuleRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.target, RoutingTarget::Direct);
}

#[test]
fn api_error_skips_none_detail() {
    let err = ApiError {
        error: "not found".to_owned(),
        detail: None,
        request_id: None,
    };
    let json = serde_json::to_string(&err).unwrap();
    assert!(!json.contains("detail"));
}

#[test]
fn api_error_includes_some_detail() {
    let err = ApiError {
        error: "bad request".to_owned(),
        detail: Some("invalid field".to_owned()),
        request_id: None,
    };
    let json = serde_json::to_string(&err).unwrap();
    assert!(json.contains("\"detail\":\"invalid field\""));
}

#[test]
fn login_request_debug_redacts_password() {
    let req = LoginRequest {
        username: "alice".to_owned(),
        password: "hunter2".to_owned(),
        remember_me: false,
    };
    let rendered = format!("{req:?}");
    assert!(rendered.contains("alice"));
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("hunter2"));
}

#[test]
fn setup_request_debug_redacts_password() {
    let req = SetupRequest {
        username: "admin".to_owned(),
        password: "super-secret".to_owned(),
    };
    let rendered = format!("{req:?}");
    assert!(rendered.contains("admin"));
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("super-secret"));
}

#[test]
fn export_backup_request_debug_redacts_passphrase() {
    let req = ExportBackupRequest {
        passphrase: "correct-horse-battery-staple".to_owned(),
    };
    let rendered = format!("{req:?}");
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("correct-horse-battery-staple"));
}

#[test]
fn update_network_zone_request_subnet_is_three_state() {
    use crate::api::UpdateNetworkZoneRequest;

    // Absent → None (leave as-is).
    let absent: UpdateNetworkZoneRequest = serde_json::from_str("{}").unwrap();
    assert_eq!(absent.subnet, None);

    // Explicit null → Some(None) (clear).
    let cleared: UpdateNetworkZoneRequest = serde_json::from_str(r#"{"subnet":null}"#).unwrap();
    assert_eq!(cleared.subnet, Some(None));

    // A value → Some(Some(..)) (set).
    let set: UpdateNetworkZoneRequest =
        serde_json::from_str(r#"{"subnet":{"cidr":"10.44.0.0/24"}}"#).unwrap();
    assert_eq!(set.subnet.unwrap().unwrap().cidr, "10.44.0.0/24");
}

/// The DNS query-stream WebSocket is not described by `OpenAPI`, so nothing else
/// pins its shape. A whole-second UTC instant must serialise as `...:56Z` —
/// the admin UI parses these, and chrono would happily emit `+00:00` or a
/// fractional part if the value carried one.
#[test]
fn query_log_event_timestamp_serialises_as_whole_second_zulu() {
    let event = crate::api::QueryLogEvent {
        timestamp: chrono::DateTime::parse_from_rfc3339("2026-05-05T12:34:56Z")
            .expect("literal is valid RFC 3339")
            .with_timezone(&chrono::Utc),
        client_ip: "10.0.0.1".to_owned(),
        domain: "example.com".to_owned(),
        query_type: "A".to_owned(),
        result: crate::dns::DnsQueryResult::Forwarded,
        upstream: None,
        latency_ms: 1.0,
        device_id: None,
    };

    let json = serde_json::to_value(&event).expect("event serialises");
    assert_eq!(json["timestamp"], "2026-05-05T12:34:56Z");
}
