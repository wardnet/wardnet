use serde::{Deserialize, Serialize};

use crate::device::{Device, DeviceType, DhcpStatus};
use crate::dhcp::{DhcpConfig, DhcpLease, DhcpReservation};
use crate::dns::{
    AllowlistEntry, Blocklist, CustomFilterRule, DnsConfig, DnsProtocol, UpstreamDns,
};
use crate::routing::RoutingTarget;
use crate::tunnel::Tunnel;
use crate::update::{InstallHandle, UpdateChannel, UpdateHistoryEntry, UpdateStatus};
use crate::vpn_provider::{
    CountryInfo, ProviderCredentials, ProviderInfo, ServerFilter, ServerInfo,
};

/// Login request body.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Login response body.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    pub message: String,
}

/// Minimal tunnel info exposed to self-service users for routing selection.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelSummary {
    pub id: String,
    pub label: String,
    pub country_code: String,
}

/// Response for GET /api/devices/me.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeviceMeResponse {
    pub device: Option<Device>,
    pub current_rule: Option<RoutingTarget>,
    pub admin_locked: bool,
    /// Available tunnels for self-service routing selection.
    pub available_tunnels: Vec<TunnelSummary>,
}

/// Request body for PUT /api/devices/me/rule.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetMyRuleRequest {
    pub target: RoutingTarget,
}

/// Response body for PUT /api/devices/me/rule.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SetMyRuleResponse {
    pub message: String,
    pub target: RoutingTarget,
}

/// Response for GET /api/system/status.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SystemStatusResponse {
    pub version: String,
    pub uptime_seconds: u64,
    pub device_count: u64,
    pub tunnel_count: u64,
    pub tunnel_active_count: u64,
    pub db_size_bytes: u64,
    pub cpu_usage_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
}

/// Response from the public info endpoint.
///
/// Returns basic server information without requiring authentication.
/// Used by the web UI's connection status widget.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InfoResponse {
    pub version: String,
    pub uptime_seconds: u64,
}

/// Standard API error response.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApiError {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Request ID for correlation with server logs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

/// Request body for POST /api/tunnels (import .conf file).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateTunnelRequest {
    pub label: String,
    pub country_code: String,
    pub provider: Option<String>,
    pub config: String,
}

/// Response for POST /api/tunnels.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateTunnelResponse {
    pub tunnel: Tunnel,
    pub message: String,
}

/// Response for GET /api/tunnels.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListTunnelsResponse {
    pub tunnels: Vec<Tunnel>,
}

/// Response for DELETE /api/tunnels/:id.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteTunnelResponse {
    pub message: String,
}

/// A device enriched with its DHCP status for API responses.
///
/// Uses `#[serde(flatten)]` so the JSON output includes all `Device` fields
/// at the top level alongside `dhcp_status`, keeping the response
/// backwards-compatible for consumers that ignore unknown fields.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DeviceWithStatus {
    #[serde(flatten)]
    pub device: Device,
    pub dhcp_status: DhcpStatus,
}

/// Response for GET /api/devices (admin).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListDevicesResponse {
    pub devices: Vec<DeviceWithStatus>,
}

/// Response for GET /api/devices/:id (admin).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeviceDetailResponse {
    pub device: DeviceWithStatus,
    pub current_rule: Option<RoutingTarget>,
}

/// Request body for PUT /api/devices/:id (admin).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateDeviceRequest {
    pub name: Option<String>,
    pub device_type: Option<DeviceType>,
    /// Routing target to set for this device (admin bypasses lock check).
    pub routing_target: Option<RoutingTarget>,
    /// Whether to lock routing changes for this device.
    pub admin_locked: Option<bool>,
}

/// Response for GET /api/setup/status.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetupStatusResponse {
    pub setup_completed: bool,
}

/// Request body for POST /api/setup.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

/// Response for POST /api/setup.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetupResponse {
    pub message: String,
}

/// Response for GET /api/providers.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListProvidersResponse {
    /// List of available VPN providers.
    pub providers: Vec<ProviderInfo>,
}

/// Request body for POST /api/providers/:id/validate.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ValidateCredentialsRequest {
    /// Credentials to validate against the provider.
    pub credentials: ProviderCredentials,
}

/// Response for POST /api/providers/:id/validate.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ValidateCredentialsResponse {
    /// Whether the credentials are valid.
    pub valid: bool,
    /// Human-readable validation result message.
    pub message: String,
}

/// Response for GET /api/providers/:id/countries.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListCountriesResponse {
    /// Available countries for this provider.
    pub countries: Vec<CountryInfo>,
}

/// Request body for POST /api/providers/:id/servers.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListServersRequest {
    /// Credentials for authenticating with the provider.
    pub credentials: ProviderCredentials,
    /// Optional filters for the server list.
    #[serde(default)]
    pub filter: ServerFilter,
}

/// Response for GET/POST /api/providers/:id/servers.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListServersResponse {
    /// List of available servers from the provider.
    pub servers: Vec<ServerInfo>,
}

/// Request body for POST /api/providers/:id/setup.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetupProviderRequest {
    /// Credentials for authenticating with the provider.
    pub credentials: ProviderCredentials,
    /// Country code for server selection (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// Optional label override; defaults to provider-generated label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// If set, use this specific server ID instead of auto-selecting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    /// Direct server hostname for dedicated IP or manual server selection.
    /// Bypasses server listing -- resolves directly by hostname.
    /// Accepts short form (`pt131`) or full (`pt131.nordvpn.com`).
    /// Takes precedence over `server_id` when both are set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
}

/// Response for POST /api/providers/:id/setup.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetupProviderResponse {
    /// The created tunnel.
    pub tunnel: Tunnel,
    /// The selected server.
    pub server: ServerInfo,
    /// Human-readable result message.
    pub message: String,
}

// ---------------------------------------------------------------------------
// DHCP API types
// ---------------------------------------------------------------------------

/// Response for GET /api/dhcp/config.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DhcpConfigResponse {
    pub config: DhcpConfig,
}

/// Request body for PUT /api/dhcp/config.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateDhcpConfigRequest {
    pub pool_start: String,
    pub pool_end: String,
    pub subnet_mask: String,
    pub upstream_dns: Vec<String>,
    pub lease_duration_secs: u32,
    pub router_ip: Option<String>,
}

/// Request body for POST /api/dhcp/config/toggle.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ToggleDhcpRequest {
    pub enabled: bool,
}

/// Response for GET /api/dhcp/leases.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListDhcpLeasesResponse {
    pub leases: Vec<DhcpLease>,
}

/// Response for GET /api/dhcp/reservations.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListDhcpReservationsResponse {
    pub reservations: Vec<DhcpReservation>,
}

/// Request body for POST /api/dhcp/reservations.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateDhcpReservationRequest {
    pub mac_address: String,
    pub ip_address: String,
    pub hostname: Option<String>,
    pub description: Option<String>,
}

/// Response for POST /api/dhcp/reservations.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateDhcpReservationResponse {
    pub reservation: DhcpReservation,
    pub message: String,
}

/// Response for DELETE /api/dhcp/reservations/:id.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeleteDhcpReservationResponse {
    pub message: String,
}

/// Response for GET /api/dhcp/status.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DhcpStatusResponse {
    pub enabled: bool,
    pub running: bool,
    pub active_lease_count: u64,
    pub pool_total: u64,
    pub pool_used: u64,
}

/// Response for DELETE /api/dhcp/leases/:id.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RevokeDhcpLeaseResponse {
    pub message: String,
}

// ---------------------------------------------------------------------------
// DNS API types
// ---------------------------------------------------------------------------

/// Response for GET /api/dns/config.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsConfigResponse {
    pub config: DnsConfig,
}

/// Request body for PUT /api/dns/config.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateDnsConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_servers: Option<Vec<UpstreamDnsRequest>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_ttl_min_secs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_ttl_max_secs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dnssec_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebinding_protection: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_second: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ad_blocking_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_log_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_log_retention_days: Option<u32>,
}

/// Upstream DNS server in API requests.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct UpstreamDnsRequest {
    pub address: String,
    pub name: String,
    pub protocol: DnsProtocol,
    pub port: Option<u16>,
}

impl From<UpstreamDnsRequest> for UpstreamDns {
    fn from(req: UpstreamDnsRequest) -> Self {
        Self {
            address: req.address,
            name: req.name,
            protocol: req.protocol,
            port: req.port,
        }
    }
}

/// Request body for POST /api/dns/config/toggle.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ToggleDnsRequest {
    pub enabled: bool,
}

/// Response for GET /api/dns/status.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DnsStatusResponse {
    pub enabled: bool,
    pub running: bool,
    pub cache_size: u64,
    pub cache_capacity: u32,
    pub cache_hit_rate: f64,
}

/// Response for POST /api/dns/cache/flush.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DnsCacheFlushResponse {
    pub message: String,
    pub entries_cleared: u64,
}

// ---------------------------------------------------------------------------
// DNS Ad Blocking — Blocklists
// ---------------------------------------------------------------------------

/// Response for GET /api/dns/blocklists.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListBlocklistsResponse {
    pub blocklists: Vec<Blocklist>,
}

/// Request body for POST /api/dns/blocklists.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateBlocklistRequest {
    pub name: String,
    pub url: String,
    pub cron_schedule: String,
    pub enabled: bool,
}

/// Response for POST /api/dns/blocklists.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateBlocklistResponse {
    pub blocklist: Blocklist,
    pub message: String,
}

/// Request body for PUT /api/dns/blocklists/{id} (partial update).
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateBlocklistRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron_schedule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Response for PUT /api/dns/blocklists/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateBlocklistResponse {
    pub blocklist: Blocklist,
    pub message: String,
}

/// Response for DELETE /api/dns/blocklists/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteBlocklistResponse {
    pub message: String,
}

// Response for POST /api/dns/blocklists/{id}/update is now
// `crate::jobs::JobDispatchedResponse` — the handler dispatches a background
// job instead of doing the fetch inline, and the client polls the job for
// progress and completion.

// ---------------------------------------------------------------------------
// DNS Ad Blocking — Allowlist
// ---------------------------------------------------------------------------

/// Response for GET /api/dns/allowlist.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListAllowlistResponse {
    pub entries: Vec<AllowlistEntry>,
}

/// Request body for POST /api/dns/allowlist.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateAllowlistRequest {
    pub domain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Response for POST /api/dns/allowlist.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateAllowlistResponse {
    pub entry: AllowlistEntry,
    pub message: String,
}

/// Response for DELETE /api/dns/allowlist/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteAllowlistResponse {
    pub message: String,
}

// ---------------------------------------------------------------------------
// DNS Ad Blocking — Custom filter rules
// ---------------------------------------------------------------------------

/// Response for GET /api/dns/rules.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListFilterRulesResponse {
    pub rules: Vec<CustomFilterRule>,
}

/// Request body for POST /api/dns/rules.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateFilterRuleRequest {
    pub rule_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    pub enabled: bool,
}

/// Response for POST /api/dns/rules.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateFilterRuleResponse {
    pub rule: CustomFilterRule,
    pub message: String,
}

/// Request body for PUT /api/dns/rules/{id} (partial update).
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateFilterRuleRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Response for PUT /api/dns/rules/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateFilterRuleResponse {
    pub rule: CustomFilterRule,
    pub message: String,
}

/// Response for DELETE /api/dns/rules/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteFilterRuleResponse {
    pub message: String,
}

// ---------------------------------------------------------------------------
// Update API types
// ---------------------------------------------------------------------------

/// Response for GET /api/update/status.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateStatusResponse {
    pub status: UpdateStatus,
}

/// Response for POST /api/update/check.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateCheckResponse {
    pub status: UpdateStatus,
}

/// Request body for POST /api/update/install.
///
/// If `version` is omitted, installs the latest known version for the current
/// channel. The operation is idempotent — calling twice while one is in
/// flight returns the same handle.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InstallUpdateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Response for POST /api/update/install.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InstallUpdateResponse {
    pub handle: InstallHandle,
    pub message: String,
}

/// Response for POST /api/update/rollback.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RollbackResponse {
    pub message: String,
}

/// Request body for PUT /api/update/config.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_update_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<UpdateChannel>,
}

/// Response for PUT /api/update/config.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateConfigResponse {
    pub status: UpdateStatus,
}

/// Response for GET /api/update/history.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateHistoryResponse {
    pub entries: Vec<UpdateHistoryEntry>,
}
