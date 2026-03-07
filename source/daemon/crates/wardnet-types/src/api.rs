use serde::{Deserialize, Serialize};

use crate::device::Device;
use crate::routing::RoutingTarget;

/// Login request body.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response body.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub message: String,
}

/// Response for GET /api/devices/me.
#[derive(Debug, Serialize)]
pub struct DeviceMeResponse {
    pub device: Option<Device>,
    pub current_rule: Option<RoutingTarget>,
    pub admin_locked: bool,
}

/// Request body for PUT /api/devices/me/rule.
#[derive(Debug, Deserialize)]
pub struct SetMyRuleRequest {
    pub target: RoutingTarget,
}

/// Response body for PUT /api/devices/me/rule.
#[derive(Debug, Serialize)]
pub struct SetMyRuleResponse {
    pub message: String,
    pub target: RoutingTarget,
}

/// Response for GET /api/system/status.
#[derive(Debug, Serialize)]
pub struct SystemStatusResponse {
    pub version: String,
    pub uptime_seconds: u64,
    pub device_count: u64,
    pub tunnel_count: u64,
    pub db_size_bytes: u64,
}

/// Standard API error response.
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_my_rule_request_deserializes_tunnel() {
        let json =
            r#"{"target":{"type":"tunnel","tunnel_id":"00000000-0000-0000-0000-000000000001"}}"#;
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
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(!json.contains("detail"));
    }

    #[test]
    fn api_error_includes_some_detail() {
        let err = ApiError {
            error: "bad request".to_owned(),
            detail: Some("invalid field".to_owned()),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"detail\":\"invalid field\""));
    }
}
