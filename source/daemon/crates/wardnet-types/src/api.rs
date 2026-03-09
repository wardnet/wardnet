use serde::{Deserialize, Serialize};

use crate::device::{Device, DeviceType};
use crate::vpn_provider::{ProviderCredentials, ProviderInfo, ServerFilter, ServerInfo};
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
    pub cpu_usage_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
}

/// Response from the public info endpoint.
///
/// Returns basic server information without requiring authentication.
/// Used by the web UI's connection status widget.
#[derive(Debug, Serialize, Deserialize)]
pub struct InfoResponse {
    pub version: String,
    pub uptime_seconds: u64,
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

/// Response for GET /api/setup/status.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetupStatusResponse {
    pub setup_completed: bool,
}

/// Request body for POST /api/setup.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

/// Response for POST /api/setup.
#[derive(Debug, Serialize, Deserialize)]
pub struct SetupResponse {
    pub message: String,
}

/// Response for GET /api/providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListProvidersResponse {
    /// List of available VPN providers.
    pub providers: Vec<ProviderInfo>,
}

/// Request body for POST /api/providers/:id/validate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateCredentialsRequest {
    /// Credentials to validate against the provider.
    pub credentials: ProviderCredentials,
}

/// Response for POST /api/providers/:id/validate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateCredentialsResponse {
    /// Whether the credentials are valid.
    pub valid: bool,
    /// Human-readable validation result message.
    pub message: String,
}

/// Request body for POST /api/providers/:id/servers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListServersRequest {
    /// Credentials for authenticating with the provider.
    pub credentials: ProviderCredentials,
    /// Optional filters for the server list.
    #[serde(default)]
    pub filter: ServerFilter,
}

/// Response for GET/POST /api/providers/:id/servers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListServersResponse {
    /// List of available servers from the provider.
    pub servers: Vec<ServerInfo>,
}

/// Request body for POST /api/providers/:id/setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupProviderRequest {
    /// Credentials for authenticating with the provider.
    pub credentials: ProviderCredentials,
    /// Country code for server selection.
    pub country: String,
    /// Optional label override; defaults to provider-generated label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// If set, use this specific server ID instead of auto-selecting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
}

/// Response for POST /api/providers/:id/setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupProviderResponse {
    /// The created tunnel.
    pub tunnel: Tunnel,
    /// The selected server.
    pub server: ServerInfo,
    /// Human-readable result message.
    pub message: String,
}
