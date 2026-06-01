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
