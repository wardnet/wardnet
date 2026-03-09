use serde::{Deserialize, Serialize};

use crate::device::{Device, DeviceType};
use crate::routing::RoutingTarget;
use crate::tunnel::Tunnel;

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

/// Request body for POST /api/tunnels (import .conf file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTunnelRequest {
    pub label: String,
    pub country_code: String,
    pub provider: Option<String>,
    pub config: String,
}

/// Response for POST /api/tunnels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTunnelResponse {
    pub tunnel: Tunnel,
    pub message: String,
}

/// Response for GET /api/tunnels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListTunnelsResponse {
    pub tunnels: Vec<Tunnel>,
}

/// Response for DELETE /api/tunnels/:id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTunnelResponse {
    pub message: String,
}

/// Response for GET /api/devices (admin).
#[derive(Debug, Serialize)]
pub struct ListDevicesResponse {
    pub devices: Vec<Device>,
}

/// Response for GET /api/devices/:id (admin).
#[derive(Debug, Serialize)]
pub struct DeviceDetailResponse {
    pub device: Device,
    pub current_rule: Option<RoutingTarget>,
}

/// Request body for PUT /api/devices/:id (admin).
#[derive(Debug, Deserialize)]
pub struct UpdateDeviceRequest {
    pub name: Option<String>,
    pub device_type: Option<DeviceType>,
}
