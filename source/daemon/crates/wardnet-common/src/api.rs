use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::backup::{BackupStatus, BundleManifest, LocalSnapshot};
use crate::device::{Device, DeviceType, DhcpStatus};
use crate::dhcp::{DhcpConfig, DhcpLease, DhcpReservation};
use crate::dns::{
    AllowlistEntry, Blocklist, CustomFilterRule, DnsConfig, DnsProtocol, DnsQueryLogEntry,
    UpstreamDns,
};
use crate::dns_filter::{DeviceDnsFilterSettings, DnsFilterConfig, DnsFilterProfile};
use crate::routing::RoutingTarget;
use crate::tunnel::Tunnel;
use crate::update::{InstallHandle, UpdateChannel, UpdateHistoryEntry, UpdateStatus};
use crate::vpn_provider::{
    CountryInfo, ProviderCredentials, ProviderInfo, ServerFilter, ServerInfo,
};
use uuid::Uuid;

/// Login request body.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

// Redact `password` so a stray `tracing::debug!(?req)` in the auth path
// can't leak the plaintext into logs.
impl std::fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRequest")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// Login response body.
///
/// `token` is the same opaque value written into the `wardnet_session` cookie;
/// non-browser clients (e.g. scripts that don't maintain a cookie jar) can
/// replay it on admin-gated requests via the `Authorization: Bearer <token>`
/// header. `expires_in_seconds` is the token's remaining lifetime measured
/// from the moment this response was generated.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LoginResponse {
    pub message: String,
    pub token: String,
    pub expires_in_seconds: u64,
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
    /// Diagnostic git-derived version. See `InfoResponse.version`.
    pub version: String,
    /// Public-facing `CalVer`. See `InfoResponse.release_version`.
    pub release_version: String,
    pub uptime_seconds: u64,
    pub device_count: u64,
    pub tunnel_count: u64,
    pub tunnel_active_count: u64,
    pub db_size_bytes: u64,
    pub cpu_usage_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    /// Classification of the previous daemon shutdown plus its
    /// acknowledgement timestamp. The web UI uses this to surface a
    /// persistent banner after an unclean shutdown until an admin
    /// dismisses it.
    pub last_shutdown: LastShutdownStatus,
}

/// How the previous daemon shutdown ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LastShutdownState {
    /// No prior shutdown has been recorded — first-ever boot.
    Unknown,
    /// Daemon recorded a graceful shutdown marker before exit.
    Graceful,
    /// Daemon was interrupted (crash, power loss, SIGKILL); the
    /// "running" marker survived into the next boot.
    Unclean,
}

/// Classification of the previous daemon shutdown.
///
/// `at` is the timestamp the previous run last touched the database
/// (graceful exit time, or last heartbeat for unclean events). It is
/// `None` only for `unknown` (first-ever boot). `acknowledged_at` is
/// set by `POST /api/system/shutdown/acknowledge`; the banner is
/// considered dismissed iff `acknowledged_at >= at`.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct LastShutdownStatus {
    pub state: LastShutdownState,
    pub at: Option<DateTime<Utc>>,
    pub acknowledged_at: Option<DateTime<Utc>>,
}

/// Response from the public info endpoint.
///
/// Returns basic server information without requiring authentication.
/// Used by the web UI's connection status widget.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InfoResponse {
    /// Diagnostic version string — git-derived
    /// `MAJOR.MINOR.PATCH[-dev.N+gHASH]`. Carries dev-suffix on
    /// non-tag builds so logs and `--version` output identify the
    /// exact commit. Use `release_version` for anything user-facing.
    pub version: String,
    /// Public-facing `CalVer` (`YYYY.MM.DD`) read from the workspace-
    /// root `CALVER` file at compile time. Stable across dev rebuilds
    /// and is the string the web UI displays, the auto-update runner
    /// compares against the published manifest, and the `OpenAPI` spec
    /// declares as `info.version`.
    pub release_version: String,
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

/// Response for GET /api/tunnels/{id}.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelDetailResponse {
    pub tunnel: Tunnel,
}

/// Range selector for `GET /api/tunnels/{id}/metrics`.
///
/// `OneHour..FortyEightHours` are served from the intraday table at
/// the configured sample interval (5 min default). `TwelveMonths`
/// reads from the daily rollup table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TunnelMetricsRange {
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "6h")]
    SixHours,
    #[serde(rename = "24h")]
    TwentyFourHours,
    #[serde(rename = "48h")]
    FortyEightHours,
    #[serde(rename = "12mo")]
    TwelveMonths,
}

impl TunnelMetricsRange {
    /// Whether the range is served from the daily rollup table.
    #[must_use]
    pub fn is_daily(self) -> bool {
        matches!(self, Self::TwelveMonths)
    }
}

/// One point on the throughput chart.
///
/// `bytes_tx` / `bytes_rx` are the *deltas* over the interval ending at
/// `ts`. The client divides by the configured sample interval (or 86400
/// for daily points) to render bytes/sec. Daily points use the start of
/// the day (UTC) as `ts`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelMetricsPoint {
    /// RFC 3339 timestamp.
    pub ts: String,
    pub bytes_tx: i64,
    pub bytes_rx: i64,
}

/// Response for `GET /api/tunnels/{id}/metrics`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelMetricsResponse {
    pub range: TunnelMetricsRange,
    /// Sample interval in seconds (5 min for intraday, 86400 for daily).
    pub interval_secs: u32,
    pub points: Vec<TunnelMetricsPoint>,
}

/// Response for `GET /api/tunnels/{id}/devices`.
///
/// The devices currently routed through the given tunnel — i.e. those
/// whose user-set or admin-set routing rule has
/// `target = Tunnel { tunnel_id: id }`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelDevicesResponse {
    pub devices: Vec<Device>,
}

/// Result payload of `POST /api/tunnels/{id}/test`.
///
/// The daemon brings the tunnel up if needed, sends a single HTTP probe
/// through the tunnel interface, and reports the exit IP, ISO-3166 alpha-2
/// country code, and round-trip latency in milliseconds.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelTestResult {
    pub tunnel_id: Uuid,
    /// Public IP observed at the tunnel exit.
    pub exit_ip: String,
    /// ISO-3166 alpha-2 country code reported by the probe service.
    pub country_code: String,
    /// Round-trip latency of the probe call, in milliseconds.
    pub latency_ms: u64,
}

/// Response envelope for `POST /api/tunnels/{id}/test`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelTestResponse {
    pub result: TunnelTestResult,
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

/// Linear stage in the first-run setup wizard.
///
/// The wizard advances `Admin → Network → Dhcp → RouterMac → Tunnel → Policy
/// → Completed`. `setup_completed` (in [`SetupStatusResponse`] and the
/// `SetupGuard` redirect logic) is derived from `wizard_step == Completed`,
/// so existing installs that already finished setup are not re-routed
/// through the new wizard after an upgrade.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WizardStep {
    /// Step 1 — create the first admin user (unauthenticated).
    Admin,
    /// Step 2 — confirm OS network state (read-only).
    Network,
    /// Step 3 — DHCP onboarding (primary or locked-router).
    Dhcp,
    /// Step 4 — discover upstream router MAC.
    RouterMac,
    /// Step 5 — first VPN tunnel (skippable).
    Tunnel,
    /// Step 6 — pick the global default routing policy.
    Policy,
    /// Step 7 — wizard finished; the dashboard takes over.
    Completed,
}

/// Branch of the DHCP onboarding flow chosen at step 3.
///
/// `Primary` runs Wardnet's built-in DHCP server. `LockedRouter` keeps the
/// upstream ISP router as the LAN's DHCP server and configures opted-in
/// devices statically with Wardnet as their gateway/DNS.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WizardMode {
    Primary,
    LockedRouter,
}

impl WizardStep {
    /// Stable lowercase identifier persisted in the `system_config` table.
    #[must_use]
    pub fn as_storage_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Network => "network",
            Self::Dhcp => "dhcp",
            Self::RouterMac => "router_mac",
            Self::Tunnel => "tunnel",
            Self::Policy => "policy",
            Self::Completed => "completed",
        }
    }

    /// Parse a `system_config` value back into a [`WizardStep`].
    ///
    /// Unknown strings are treated as a corrupted DB and fall back to
    /// [`WizardStep::Admin`] so the user re-enters the wizard rather than
    /// landing in an undefined state.
    #[must_use]
    pub fn from_storage_str(s: &str) -> Self {
        match s {
            "network" => Self::Network,
            "dhcp" => Self::Dhcp,
            "router_mac" => Self::RouterMac,
            "tunnel" => Self::Tunnel,
            "policy" => Self::Policy,
            "completed" => Self::Completed,
            _ => Self::Admin,
        }
    }

    /// Linear ordinal used for "no going backwards" validation.
    #[must_use]
    pub fn ordinal(&self) -> u8 {
        match self {
            Self::Admin => 0,
            Self::Network => 1,
            Self::Dhcp => 2,
            Self::RouterMac => 3,
            Self::Tunnel => 4,
            Self::Policy => 5,
            Self::Completed => 6,
        }
    }
}

impl WizardMode {
    /// Stable lowercase identifier persisted in the `system_config` table.
    #[must_use]
    pub fn as_storage_str(&self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::LockedRouter => "locked_router",
        }
    }

    /// Parse a `system_config` value back into a [`WizardMode`].
    /// Unknown strings yield `None`.
    #[must_use]
    pub fn from_storage_str(s: &str) -> Option<Self> {
        match s {
            "primary" => Some(Self::Primary),
            "locked_router" => Some(Self::LockedRouter),
            _ => None,
        }
    }
}

/// Response for GET /api/setup/status.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetupStatusResponse {
    /// Derived: `wizard_step == Completed`. Kept for `SetupGuard`
    /// backwards-compat — clients that haven't been updated still get a
    /// boolean they understand.
    pub setup_completed: bool,
    pub wizard_step: WizardStep,
    /// `None` until step 3 picks a branch.
    pub wizard_mode: Option<WizardMode>,
}

/// Request body for POST /api/setup/advance.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AdvanceWizardRequest {
    pub to_step: WizardStep,
    /// Set when transitioning into [`WizardStep::Dhcp`] so the daemon
    /// knows which onboarding branch to take.
    pub wizard_mode: Option<WizardMode>,
}

/// Response for POST /api/setup/advance.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AdvanceWizardResponse {
    pub wizard_step: WizardStep,
    pub wizard_mode: Option<WizardMode>,
}

/// Request body for POST /api/setup.
#[derive(Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

/// Request body for PUT /api/system/default-policy.
///
/// `policy` is either the literal string `"direct"` or a tunnel UUID.
/// The service layer validates the format and rejects anything else.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetDefaultPolicyRequest {
    pub policy: String,
}

/// How the LAN interface acquired its current IP address.
///
/// Returned by `GET /api/network/status` so the wizard's network step
/// can show a remediation panel when the host is still relying on
/// DHCP — install.sh writes `/etc/dhcpcd.conf.d/wardnet.conf` when the
/// operator passes `--static-ip`, which flips this to `Static`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DhcpSource {
    /// `/etc/dhcpcd.conf.d/wardnet.conf` is present — install.sh pinned
    /// the address.
    Static,
    /// No Wardnet drop-in found — the host is using whatever the
    /// upstream router handed out.
    Dhcp,
    /// Couldn't determine. Treated like `Dhcp` for remediation purposes
    /// but called out separately so the operator knows we're unsure.
    Unknown,
}

/// Response for GET /api/network/status.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NetworkStatusResponse {
    pub interface: String,
    #[schema(value_type = String)]
    pub ip: std::net::Ipv4Addr,
    #[schema(value_type = String)]
    pub gateway: Option<std::net::Ipv4Addr>,
    pub dhcp_source: DhcpSource,
}

/// How the upstream router MAC was obtained.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouterMacSource {
    /// Discovered via an ARP probe at the gateway address.
    Arp,
    /// Operator typed it into the wizard's manual-entry field.
    Manual,
}

/// Request body for POST /api/network/discover-gateway-mac.
///
/// All fields optional. If `mac` is provided the daemon skips the ARP
/// probe and just persists the value (validated). Otherwise it probes
/// the gateway IP from `GET /api/network/status`; `target_ip` lets a
/// caller override that for testing or when the gateway differs from
/// the kernel's default route.
#[derive(Debug, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiscoverGatewayMacRequest {
    pub mac: Option<String>,
    #[schema(value_type = String)]
    pub target_ip: Option<std::net::Ipv4Addr>,
}

/// Response for POST /api/network/discover-gateway-mac.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiscoverGatewayMacResponse {
    pub mac: String,
    pub source: RouterMacSource,
}

/// Response for POST /api/network/dhcp-self-probe.
///
/// The wizard's primary-mode step 3 uses this to verify Wardnet now
/// owns LAN DHCP after the operator disabled it on their router:
///
/// - `wardnet_responded == true && foreign_responded == false` → ready
///   to advance.
/// - `foreign_responded == true` → re-show the disable-DHCP guide;
///   `foreign_server_ip` lets the wizard call out which device is
///   still answering.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DhcpSelfProbeResponse {
    pub wardnet_responded: bool,
    pub foreign_responded: bool,
    #[schema(value_type = String)]
    pub foreign_server_ip: Option<std::net::Ipv4Addr>,
}

/// Response for PUT /api/system/default-policy.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetDefaultPolicyResponse {
    pub policy: String,
}

// Redact `password` — same rationale as `LoginRequest`.
impl std::fmt::Debug for SetupRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetupRequest")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
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
    pub dns_filtering_enabled: Option<bool>,
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
// DNS Filter — Profiles
// ---------------------------------------------------------------------------

/// Response for GET /api/dns/filter/profiles.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListProfilesResponse {
    pub profiles: Vec<DnsFilterProfile>,
}

/// Response for GET /api/dns/filter/profiles/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GetProfileResponse {
    pub profile: DnsFilterProfile,
}

/// Request body for POST /api/dns/filter/profiles.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateProfileRequest {
    pub name: String,
}

/// Response for POST /api/dns/filter/profiles.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateProfileResponse {
    pub profile: DnsFilterProfile,
    pub message: String,
}

/// Request body for PUT /api/dns/filter/profiles/{id}.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateProfileRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Response for PUT /api/dns/filter/profiles/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateProfileResponse {
    pub profile: DnsFilterProfile,
    pub message: String,
}

/// Response for DELETE /api/dns/filter/profiles/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteProfileResponse {
    pub message: String,
}

// ---------------------------------------------------------------------------
// DNS Filter — Per-device settings
// ---------------------------------------------------------------------------

/// Response for GET /api/dns/filter/devices/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GetDeviceFilterSettingsResponse {
    pub settings: DeviceDnsFilterSettings,
}

/// Request body for PUT /api/dns/filter/devices/{id}.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateDeviceFilterSettingsRequest {
    pub enabled: bool,
    pub profile_ids: Vec<Uuid>,
}

/// Response for PUT /api/dns/filter/devices/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateDeviceFilterSettingsResponse {
    pub settings: DeviceDnsFilterSettings,
    pub message: String,
}

/// Response for GET /api/dns/filter/devices.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListDeviceFilterSettingsResponse {
    pub devices: Vec<DeviceDnsFilterSettings>,
}

/// Query params for GET /api/dns/filter/devices.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
pub struct ListDeviceFilterSettingsParams {
    /// When `Some(false)`, restrict to devices where the kill switch is off.
    /// When `None` (default), return every device with explicit settings or
    /// profile assignments.
    #[serde(default)]
    pub enabled: Option<bool>,
}

// ---------------------------------------------------------------------------
// DNS Filter — Global config
// ---------------------------------------------------------------------------

/// Response for GET /api/dns/filter/config.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsFilterConfigResponse {
    pub config: DnsFilterConfig,
}

/// Request body for PUT /api/dns/filter/config.
///
/// `default_profile_id` uses double-`Option` semantics: omitted from JSON =
/// no change, `null` = clear the default, `"<uuid>"` = set.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateDnsFilterConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile_id: Option<Option<Uuid>>,
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

// ---------------------------------------------------------------------------
// Backup / restore
// ---------------------------------------------------------------------------

/// Request body for `POST /api/backup/export`.
///
/// The passphrase is used to derive the age encryption key via scrypt.
/// Server-side we enforce a minimum length of
/// `crate::backup::MIN_PASSPHRASE_LEN`; clients should surface the same
/// minimum in UI copy so failures happen before the request.
#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExportBackupRequest {
    /// Passphrase chosen by the admin. Never logged, never persisted —
    /// lives only in the memory of the request that produced the bundle.
    pub passphrase: String,
}

// Manual `Debug` so that a future `tracing::debug!(?req)` anywhere in
// the export path renders `passphrase: "[REDACTED]"` instead of the
// plaintext. The derived impl would leak it unconditionally.
impl std::fmt::Debug for ExportBackupRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExportBackupRequest")
            .field("passphrase", &"[REDACTED]")
            .finish()
    }
}

/// Response for `POST /api/backup/import/preview`.
///
/// Returned before any daemon state is touched. The UI uses this to show
/// the admin what will change — which database, config, and key files get
/// swapped, and which bundle they came from — so the actual apply step
/// is a conscious confirmation.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RestorePreviewResponse {
    /// Bundle manifest, fully decrypted and validated.
    pub manifest: BundleManifest,
    /// True when the bundle's `bundle_format_version` and
    /// `schema_version` are both compatible with the running daemon. A
    /// `false` here means `apply_import` will refuse; the UI should
    /// surface `incompatibility_reason` and hide the confirm button.
    pub compatible: bool,
    /// Human-readable reason the bundle is incompatible, when
    /// `compatible` is `false`. `None` on the happy path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incompatibility_reason: Option<String>,
    /// Files the apply step will rename to `.bak-<timestamp>` siblings
    /// and then overwrite. Surfaced verbatim in the UI so operators
    /// understand the blast radius before confirming.
    pub files_to_replace: Vec<String>,
    /// Opaque token the caller passes back to `apply_import` to prove
    /// they just saw this preview. Scoped to a single bundle + session.
    pub preview_token: String,
}

/// Request body for `POST /api/backup/import/apply`.
///
/// Consumes the `preview_token` returned by `/preview`, ensuring the
/// apply step always has a prior preview (no silent blind restores).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApplyImportRequest {
    pub preview_token: String,
}

/// Response for `POST /api/backup/import/apply`.
///
/// Returned once the swap completes. The daemon restarts subsystems
/// (DHCP/DNS/update runners) after the swap; subsequent API calls see
/// the restored state.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApplyImportResponse {
    /// Final manifest for the applied bundle — identical to the one
    /// surfaced in the preview but repeated here so the UI can confirm
    /// without re-fetching.
    pub manifest: BundleManifest,
    /// `.bak-<timestamp>` snapshots of the files that were replaced.
    /// Retained by the background cleanup task for 24 h.
    pub snapshots: Vec<LocalSnapshot>,
}

/// Response for `GET /api/backup/status`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BackupStatusResponse {
    pub status: BackupStatus,
}

/// Response for `GET /api/backup/snapshots`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListSnapshotsResponse {
    pub snapshots: Vec<LocalSnapshot>,
}

// ---------------------------------------------------------------------------
// DNS query log + stats
// ---------------------------------------------------------------------------

/// Query parameters for `GET /api/dns/log`.
#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams)]
pub struct ListQueryLogParams {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub client_ip: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
}

/// Response for `GET /api/dns/log`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListQueryLogResponse {
    pub entries: Vec<DnsQueryLogEntry>,
    pub total: u64,
}

/// Live event broadcast over `/api/dns/log/stream`. Mirrors a row in
/// `dns_query_log` so clients can render entries before they're persisted.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QueryLogEvent {
    /// RFC 3339 timestamp.
    pub timestamp: String,
    pub client_ip: String,
    pub domain: String,
    pub query_type: String,
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    pub latency_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// Query parameters for `GET /api/dns/stats`.
#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams)]
pub struct DnsStatsParams {
    /// Window in hours, default 24, max 168 (7 days).
    #[serde(default)]
    pub hours: Option<u32>,
}

/// Aggregate counters for the requested window.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsStatsTotals {
    pub total_queries: u64,
    pub blocked_queries: u64,
    pub blocked_percent: f64,
    pub avg_latency_ms: f64,
    pub unique_clients: u64,
    pub unique_domains: u64,
}

/// Top domain by hit count.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TopDomain {
    pub domain: String,
    pub count: u64,
}

/// Top client by query count, optionally enriched with device metadata.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TopClient {
    pub client_ip: String,
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_mac: Option<String>,
}

/// One point on the queries-over-time chart.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsSeriesPoint {
    /// Bucket label: `YYYY-MM-DD HH:MM` (minute) or `YYYY-MM-DD HH` (hour).
    pub bucket: String,
    pub total: u64,
    pub blocked: u64,
}

/// Bucket size for the series chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DnsSeriesBucket {
    Minute,
    Hour,
}

/// Response for `GET /api/dns/stats`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsStatsResponse {
    pub hours: u32,
    pub totals: DnsStatsTotals,
    pub top_domains: Vec<TopDomain>,
    pub top_blocked: Vec<TopDomain>,
    pub top_clients: Vec<TopClient>,
    pub series_bucket: DnsSeriesBucket,
    pub series: Vec<DnsSeriesPoint>,
}
