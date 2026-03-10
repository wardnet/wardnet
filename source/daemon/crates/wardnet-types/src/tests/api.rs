use crate::api::{ApiError, SetMyRuleRequest};
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
