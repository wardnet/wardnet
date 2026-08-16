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

#[test]
fn set_device_owner_request_treats_an_absent_field_as_clear() {
    use crate::api::SetDeviceOwnerRequest;

    // Unlike `UpdateNetworkZoneRequest` above, this is **two**-state, not
    // three: a `PUT` replaces the owner outright, so there is no "leave as-is"
    // to distinguish. That matters because the Go SDK's generated body tags
    // the field `omitempty`, so clearing an owner goes out as `{}` rather than
    // an explicit null — and both must mean the same thing, or the SDK would
    // silently no-op instead of unassigning.
    let absent: SetDeviceOwnerRequest = serde_json::from_str("{}").unwrap();
    assert_eq!(absent.owner_user_id, None);

    let null: SetDeviceOwnerRequest = serde_json::from_str(r#"{"owner_user_id":null}"#).unwrap();
    assert_eq!(null.owner_user_id, None);

    let set: SetDeviceOwnerRequest =
        serde_json::from_str(r#"{"owner_user_id":"6e05df45-1fa4-4327-8c1e-218c79b253ba"}"#)
            .unwrap();
    assert_eq!(
        set.owner_user_id.map(|u| u.to_string()).as_deref(),
        Some("6e05df45-1fa4-4327-8c1e-218c79b253ba")
    );
}
