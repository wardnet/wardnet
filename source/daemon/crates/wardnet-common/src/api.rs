use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::anomaly::{Anomaly, AnomalyQueryStatus, AnomalySeverity, AnomalyType};
use crate::auth::UserRole;
use crate::backup::{BackupStatus, BundleManifest, LocalSnapshot};
use crate::device::{Device, DeviceSignal, DeviceType, DhcpStatus};
use crate::dhcp::{DhcpConfig, DhcpLease, DhcpReservation};
use crate::dns::{
    AllowlistEntry, Blocklist, ConditionalForwardingRule, CustomDnsRecord, CustomFilterRule,
    DnsConfig, DnsProtocol, DnsQueryLogEntry, DnsQueryResult, DnsRecordSource, DnsRecordType,
    DnsResolutionMode, DnsZone, ForwarderSelectionMode, UpstreamDns, UpstreamLatency,
};
use crate::dns_filter::{DeviceDnsFilterSettings, DnsFilterConfig, DnsFilterProfile};
use crate::network_zone::{AllowedTargetKind, NetworkZone, ZoneStance, ZoneSubnet};
use crate::routing::RoutingTarget;
use crate::routing_profile::{DomainRoutingRule, DomainRoutingTarget, RoutingProfile};
use crate::tunnel::{BestServerSelector, Tunnel, TunnelStatus};
use crate::update::{InstallHandle, UpdateChannel, UpdateHistoryEntry, UpdateStatus};
use crate::vpn_provider::{
    CountryInfo, ProviderCredentials, ProviderInfo, ServerFilter, ServerInfo,
};
use crate::zone_exception::{ExceptionEndpoint, ServiceSpec, ZoneException};
use uuid::Uuid;

/// Login request body.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    /// When `true`, the session cookie is set with a 30-day `Max-Age` instead
    /// of the default 24-hour expiry. Admin-app always sends `true`; admin-site
    /// exposes this as a "Remember me" checkbox.
    #[serde(default)]
    #[schema(default = false)]
    pub remember_me: bool,
}

// Redact `password` so a stray `tracing::debug!(?req)` in the auth path
// can't leak the plaintext into logs.
impl std::fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginRequest")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("remember_me", &self.remember_me)
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

/// Response for GET /api/users/me.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MeResponse {
    /// The authenticated user's display name.
    ///
    /// Kept under the name `username` **additively** (ADR-0031 §8): existing
    /// clients — the setup wizard's review step among them — read this field,
    /// and for a backfilled local admin it is exactly the old
    /// `admins.username`. New clients should prefer `display_name`, which is
    /// the same value under an honest name.
    pub username: String,
    /// The authenticated user's id.
    pub id: String,
    /// The authenticated user's display name. Same value as `username`.
    pub display_name: String,
    /// Optional email address. `None` for a local admin created by the wizard
    /// or the config-file bootstrap, neither of which asks for one.
    pub email: Option<String>,
    /// `admin` or `member`. Lets a UI hide admin-only surfaces without probing
    /// endpoints for 403s.
    pub role: UserRole,
}

/// Minimal tunnel info exposed to self-service users for routing selection.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelSummary {
    pub id: String,
    pub label: String,
    pub country_code: String,
    pub status: TunnelStatus,
    pub last_handshake: Option<DateTime<Utc>>,
}

/// Minimal Network Zone info exposed to a self-service caller for the
/// read-only zone display in the user PWA.
///
/// The caller is device-keyed and has no admin session, so it cannot call the
/// admin-gated `GET /api/network/zones`. `GET /api/devices/me` resolves the
/// caller's own zone (via an internal admin context) and hands back just the
/// name plus whether it is the anchor "home" zone — enough for
/// "You're on: Guest — isolated from the home network" without leaking the
/// full zone policy.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ZoneSummary {
    /// Human-readable zone name (e.g. "Guest").
    pub name: String,
    /// Whether this is the protected anchor "home" zone. When `false` the
    /// device sits in a non-home zone that is isolated from it.
    pub is_default: bool,
}

/// Minimal routing-profile info exposed to a self-service caller for the
/// read-only "profiles applied to your device" display in the user PWA.
///
/// Like [`ZoneSummary`], the caller is device-keyed and cannot call the
/// admin-gated `GET /api/routing/profiles`; `GET /api/devices/me` resolves the
/// caller's assigned profiles (via an internal admin context) and hands back
/// just id + name — enough to show "Netflix → UK is routing some of your
/// traffic" without exposing the rules or letting the user edit them.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RoutingProfileSummary {
    pub id: String,
    /// Human-readable profile name (e.g. "Streaming (UK)").
    pub name: String,
}

/// Response for GET /api/devices/me.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeviceMeResponse {
    pub device: Option<Device>,
    pub current_rule: Option<RoutingTarget>,
    pub admin_locked: bool,
    /// Available tunnels for self-service routing selection.
    pub available_tunnels: Vec<TunnelSummary>,
    /// The caller device's Network Zone, resolved server-side for the
    /// read-only zone display. `None` if the device or its zone can't be
    /// resolved.
    pub zone: Option<ZoneSummary>,
    /// Routing profiles assigned to the caller device, in priority order.
    /// Read-only — a device-keyed caller cannot manage profiles. Empty when
    /// none are assigned.
    #[serde(default)]
    pub routing_profiles: Vec<RoutingProfileSummary>,
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
    pub disk_free_bytes: u64,
    pub disk_total_bytes: u64,
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
    /// Whether this box is currently entitled to the premium app surfaces
    /// (the user PWA and admin mobile app) — on the wardnet DDNS provider and
    /// not suspended. The web UI uses this to self-gate an already-installed
    /// PWA served from its service-worker precache, which never re-hits the
    /// server-side serving gate.
    pub entitled: bool,
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
    /// Set when the tunnel was created via country-scoped auto-select.
    pub server_selector: Option<BestServerSelector>,
    /// Human-readable name of the server resolved at creation time.
    pub resolved_server_name: Option<String>,
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

/// Response for POST /api/tunnels/:id/rebuild.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RebuildTunnelResponse {
    /// Always `true` on a 200 response. Errors surface as non-200 status codes.
    pub ok: bool,
}

/// Response for GET /api/tunnels/{id}.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelDetailResponse {
    pub tunnel: Tunnel,
}

/// Request body for `PUT /api/tunnels/{id}/dns-override`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateTunnelDnsOverrideRequest {
    /// `true` routes tunneled-device DNS through wardnet (so the
    /// ad-blocking filter still runs) using the tunnel's DNS server as
    /// upstream with `SO_BINDTODEVICE`. `false` falls back to the
    /// system-wide upstream pool. See issue #342.
    pub override_default_dns: bool,
}

/// Response for `PUT /api/tunnels/{id}/dns-override`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateTunnelDnsOverrideResponse {
    pub tunnel: Tunnel,
}

// -- Inbound (multi-peer) WireGuard server (issue #809) -------------------

/// Request body for `PUT /api/inbound-wg/config`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InboundWgConfigRequest {
    /// Whether the inbound `WireGuard` server should be running.
    pub enabled: bool,
    /// UDP port the server listens on for inbound peer handshakes.
    pub listen_port: u16,
}

/// Response for `PUT /api/inbound-wg/config`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InboundWgConfigResponse {
    /// The applied enabled state.
    pub enabled: bool,
    /// The applied listen port.
    pub listen_port: u16,
    /// The server's public key once a keypair exists (generated on first
    /// enable). `None` until the server has been enabled at least once.
    pub server_public_key: Option<String>,
}

/// Request body for `POST /api/inbound-wg/peers`.
///
/// A remote-access grant targets an already-managed device (issue #810): the
/// peer's user-facing name is taken from that device, not supplied here.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AddInboundWgPeerRequest {
    /// The device to grant remote access to. Must already exist (discovered on
    /// the LAN at least once) and must not already have a credential.
    pub device_id: Uuid,
}

/// Response for `POST /api/inbound-wg/peers`.
///
/// Carries the complete, ready-to-import `WireGuard` client configuration —
/// assembled server-side and containing the freshly generated **private key**
/// exactly once. The daemon never persists the private key (it lives only in
/// this response); the admin must copy/scan it now. This is the ONLY endpoint
/// that ever exposes private key material, and it is admin-gated.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AddInboundWgPeerResponse {
    pub id: Uuid,
    pub name: String,
    /// Base64 `WireGuard` public key (stored server-side).
    pub public_key: String,
    /// The peer's allocated `/32` inside the inbound tunnel subnet.
    pub allowed_ip: String,
    /// The full `WireGuard` client config (`.conf`) — `[Interface]`/`[Peer]`
    /// stanzas including the private key and endpoint — ready to render as a QR
    /// code or download. `None` when no reachable endpoint is known yet (remote
    /// access / DDNS not configured); the credential is created but not usable
    /// until an endpoint exists.
    pub client_config: Option<String>,
}

/// A single inbound-`WireGuard` peer, without any private key material.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InboundWgPeerSummary {
    pub id: Uuid,
    pub name: String,
    pub public_key: String,
    pub allowed_ip: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    /// The `Device` this credential grants remote access to. `None` only for
    /// pre-#810 rows written before the device link existed; every row
    /// written since always carries it.
    pub device_id: Option<Uuid>,
}

/// Response for `GET /api/inbound-wg/peers`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListInboundWgPeersResponse {
    pub peers: Vec<InboundWgPeerSummary>,
}

/// Request body for `PATCH /api/inbound-wg/peers/{id}`.
///
/// Pauses or resumes a peer without deleting its credential — distinct from
/// `DELETE`, which revokes it permanently and requires a fresh keypair (and
/// QR scan) to re-grant.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetInboundWgPeerEnabledRequest {
    pub enabled: bool,
}

// -- Private DNS (encrypted DNS at home + roaming, issue #914) -------------

/// Enable prerequisites for Private DNS, surfaced to the admin UI so it can
/// explain *why* the toggle is blocked.
///
/// The first three are **hard gates** — [`PrivateDnsStatusResponse::enabled`]
/// cannot be turned on unless all are `true`. `relay_connected` is
/// **informational**: the LAN (split-horizon) path works without the reverse
/// tunnel; only roaming needs it, so a disconnected relay never blocks enable.
#[allow(clippy::struct_excessive_bools)] // four independent prerequisite flags, not a state machine
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PrivateDnsPrerequisites {
    /// The box is on the wardnet DDNS provider (so grants get a wildcard-SAN
    /// hostname).
    pub wardnet_provider: bool,
    /// An issued TLS certificate is live (the `DoT` listener derives its config
    /// from it).
    pub certificate: bool,
    /// The box currently holds an active Premium entitlement.
    pub entitled: bool,
    /// The reverse tunnel to the regional edge is up. Informational only —
    /// roaming queries need it, LAN queries do not.
    pub relay_connected: bool,
}

/// One per-device grant, carrying the full minted hostname the phone dials.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PrivateDnsGrantSummary {
    pub id: Uuid,
    pub device_id: Uuid,
    /// The device's secret hostname (`<token>.<domain>`) — entered into
    /// Android Private DNS and carried as the `ServerName` in the iOS profile.
    pub hostname: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}

/// Response for `GET /api/private-dns/status` and `PUT /api/private-dns`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PrivateDnsStatusResponse {
    /// Whether the feature (and therefore the `:853` listener) is enabled.
    pub enabled: bool,
    /// The wardnet FQDN grants live under (`<token>.<domain>`), when the
    /// provider is configured; `null` otherwise.
    pub domain: Option<String>,
    pub prerequisites: PrivateDnsPrerequisites,
    /// Every grant, oldest first.
    pub grants: Vec<PrivateDnsGrantSummary>,
}

/// Request body for `PUT /api/private-dns`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetPrivateDnsEnabledRequest {
    pub enabled: bool,
}

/// Request body for `POST /api/private-dns/grants`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreatePrivateDnsGrantRequest {
    /// The device to grant encrypted-DNS access. Must already exist and must
    /// not already hold a grant.
    pub device_id: Uuid,
}

/// Response for `GET /api/private-dns/me` — the device-keyed self-service view
/// (identified by source IP, like `GET /api/devices/me`).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PrivateDnsMeResponse {
    /// Whether Private DNS is enabled on the box at all.
    pub enabled: bool,
    /// Whether *this* device holds a grant.
    pub granted: bool,
    /// This device's full hostname, present only when granted and the wardnet
    /// domain is known; `null` otherwise.
    pub hostname: Option<String>,
}

/// Response for `POST /api/private-dns/grants/{device_id}/notify` — the result
/// of nudging a granted device's household member to set up Private DNS.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SendPrivateDnsNotificationResponse {
    /// Whether at least one push subscription for the device was targeted.
    /// `false` (not an error) when the device holds a grant but the household
    /// member hasn't enabled notifications, so no subscription exists.
    pub delivered: bool,
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

/// A device enriched with its DHCP status and current routing target for API
/// responses.
///
/// Uses `#[serde(flatten)]` so the JSON output includes all `Device` fields
/// at the top level alongside `dhcp_status` and `current_rule`, keeping the
/// response backwards-compatible for consumers that ignore unknown fields.
///
/// `current_rule` mirrors [`DeviceDetailResponse::current_rule`]: `None` means
/// the device has no rule of its own and follows the gateway default policy;
/// `Some(RoutingTarget::Default)` is an explicit persisted default choice.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DeviceWithStatus {
    #[serde(flatten)]
    pub device: Device,
    pub dhcp_status: DhcpStatus,
    pub current_rule: Option<RoutingTarget>,
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
    /// Identification signals observed for this device, most recent first
    /// (issue #1099). Empty is the normal case for a device that has only ever
    /// been seen by ARP — it is an absence of evidence, not a failure.
    pub signals: Vec<DeviceSignal>,
}

/// Response for POST /api/devices/:id/identify (admin, issue #1116).
///
/// Carries what the probe *contacted* as well as what answered, so the UI can
/// say "we tried these four ports and none answered" rather than leaving the
/// identification card visually unchanged and the button looking broken. An
/// empty `answering_ports` is a real, informative result.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DeviceProbeResponse {
    /// Every TCP port the probe contacted — the vendor catalog's full probe
    /// surface, and by construction its upper bound.
    pub ports_probed: Vec<u16>,
    /// The subset that answered. Each was recorded as a `probed_port`
    /// identification signal and may have named the device.
    pub answering_ports: Vec<u16>,
    /// The device re-read after the probe, so the caller sees any manufacturer
    /// and signals the probe just produced without a second round trip.
    pub device: DeviceDetailResponse,
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
/// The wizard advances `Admin → Network → Dhcp → RouterMac → Dns → Tunnel →
/// Policy → RemoteAccess → Review → Completed`. `setup_completed` (in
/// [`SetupStatusResponse`] and the `SetupGuard` redirect logic) is derived
/// from `wizard_step == Completed`, so existing installs that already
/// finished setup are not re-routed through the new wizard after an upgrade.
///
/// The wizard may also rewind: any step down to [`Self::Network`] can be
/// revisited (never [`Self::Admin`] — admin creation is one-shot), except
/// once [`Self::Completed`] is reached, which is terminal.
///
/// Steps are persisted by their stable storage string (not their ordinal), so
/// inserting [`Self::Dns`] and [`Self::Review`] mid-sequence needs no
/// migration: finished installs (`"completed"`) and in-progress ones keep
/// resolving to the same variant.
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
    /// Step 5 — pick the baseline DNS-filter profiles applied to devices
    /// without an explicit assignment.
    Dns,
    /// Step 6 — first VPN tunnel (skippable).
    Tunnel,
    /// Step 7 — pick the global default routing policy.
    Policy,
    /// Step 8 — enable remote access (HTTPS): register a public hostname via
    /// the bridge or BYOD-Cloudflare and issue a certificate. Skippable.
    RemoteAccess,
    /// Step 9 — read-only summary of everything configured so far.
    Review,
    /// Step 10 — wizard finished; the dashboard takes over.
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
            Self::Dns => "dns",
            Self::Tunnel => "tunnel",
            Self::Policy => "policy",
            Self::RemoteAccess => "remote_access",
            Self::Review => "review",
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
            "dns" => Self::Dns,
            "tunnel" => Self::Tunnel,
            "policy" => Self::Policy,
            "remote_access" => Self::RemoteAccess,
            "review" => Self::Review,
            "completed" => Self::Completed,
            _ => Self::Admin,
        }
    }

    /// Linear ordinal used for rewind validation (floor is [`Self::Network`];
    /// [`Self::Completed`] is terminal).
    #[must_use]
    pub fn ordinal(&self) -> u8 {
        match self {
            Self::Admin => 0,
            Self::Network => 1,
            Self::Dhcp => 2,
            Self::RouterMac => 3,
            Self::Dns => 4,
            Self::Tunnel => 5,
            Self::Policy => 6,
            Self::RemoteAccess => 7,
            Self::Review => 8,
            Self::Completed => 9,
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

/// Overall health verdict surfaced by `GET /health` (issue #214).
///
/// Maps to the HTTP status: `Up` → 200, `Down` → 503. Mirrors the
/// `HealthStatus` produced by the daemon's `HealthMonitor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HealthStatusDto {
    /// All registered checks are passing (after debounce).
    Up,
    /// At least one component is down.
    Down,
}

/// One component's debounced status within a [`HealthResponse`].
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct HealthComponentDto {
    /// Stable check name (e.g. `"database"`, `"dns"`, `"dhcp"`, `"liveness"`).
    pub name: String,
    /// Debounced status of this component.
    pub status: HealthStatusDto,
    /// Failure detail, present only while the component is `DOWN`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Response body for `GET /health` (issue #214).
///
/// Unauthenticated, Actuator/k8s-style. The HTTP status code carries the
/// machine-readable verdict (200 UP / 503 DOWN); the body adds the per-
/// component breakdown for humans and dashboards.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct HealthResponse {
    /// Overall verdict — `DOWN` if any component is down.
    pub status: HealthStatusDto,
    /// Per-component statuses, in registration order.
    pub components: Vec<HealthComponentDto>,
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

// ── Remote access: DDNS + TLS (issues #527–#530) ─────────────────────────────

/// Response for `GET /api/ddns/check`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DdnsCheckResponse {
    /// `true` when the slug is well-formed and unclaimed.
    pub available: bool,
}

/// Request body for `POST /api/ddns/enrollment-code` (wardnet provider, step 1).
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DdnsEnrollmentCodeRequest {
    /// The wardnet account email a one-time enrollment code is emailed to.
    pub email: String,
}

/// Request body for `POST /api/ddns/enroll` (wardnet provider, step 2).
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DdnsEnrollRequest {
    /// The one-time code emailed to the account, as entered by the operator.
    pub code: String,
}

/// Request body for `POST /api/ddns/register` (wardnet provider, final step).
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DdnsRegisterRequest {
    /// The vanity slug to claim, e.g. `happy-einstein`, forming
    /// `<slug>.my.wardnet.services`. The cloud validates it (3–32 chars,
    /// `[a-z0-9-]`, no leading/trailing hyphen, not reserved).
    pub slug: String,
    /// Optional human-facing network name; defaults to the slug when omitted.
    #[serde(default)]
    pub display_name: Option<String>,
}

/// Request body for `POST /api/ddns/cloudflare` (BYOD provider).
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ConfigureCloudflareRequest {
    /// A Cloudflare API token scoped to DNS:Edit on the domain's zone.
    pub token: String,
    /// The fully-qualified domain the operator controls, e.g. `home.example.com`.
    pub domain: String,
}

/// Response for `POST /api/ddns/register` and `POST /api/ddns/cloudflare`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DdnsRegisterResponse {
    /// The public hostname now assigned to this network.
    pub fqdn: String,
    /// The region slug (display only); `None` for BYOD-Cloudflare.
    pub region: Option<String>,
    /// `true` when the daemon JOINED an already-existing network of this
    /// account (same slug, re-enrollment) instead of creating a fresh one —
    /// the network's region and name won over the request (display only).
    #[serde(default)]
    pub adopted: bool,
}

/// Response for `GET /api/ddns/status`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DdnsStatusResponse {
    /// `None` when DDNS is not configured; otherwise `"wardnet"` or `"cloudflare"`.
    pub provider: Option<String>,
    /// The active public hostname (wardnet subdomain or BYOD domain), if any.
    pub fqdn: Option<String>,
    /// The IP last published by the daemon, if any.
    pub last_public_ip: Option<String>,
    /// `true` when the wardnet subscription is suspended — the premium app
    /// surfaces (user PWA + admin mobile app) are disabled until it is restored.
    /// Always `false` for BYOD-Cloudflare.
    #[serde(default)]
    pub suspended: bool,
}

/// Verdict of the external [resolution check](DdnsResolutionCheckResponse):
/// whether the *public* internet resolves the canonical FQDN to the IP the
/// daemon last published. Deliberately bypasses the local split-horizon override.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DdnsResolutionVerdict {
    /// DDNS is not configured — there is nothing to resolve.
    NotConfigured,
    /// Public DNS resolves the FQDN to the published IP — propagation complete.
    Match,
    /// Public DNS resolves the FQDN, but to a different IP (stale record or a
    /// recent WAN-IP change not yet propagated).
    Mismatch,
    /// No A record is visible publicly yet — the normal state during the
    /// propagation window right after registration.
    Pending,
}

/// Response for `GET /api/ddns/resolution-check`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DdnsResolutionCheckResponse {
    /// Overall verdict.
    pub verdict: DdnsResolutionVerdict,
    /// The canonical FQDN that was queried, if DDNS is configured.
    pub fqdn: Option<String>,
    /// The IP the daemon last published (the value public DNS is expected to
    /// return), if any.
    pub expected_ip: Option<String>,
    /// The A records the external resolver actually returned for `fqdn`.
    pub resolved_ips: Vec<String>,
}

/// Coarse stage of the daemon's TLS-certificate provisioning, surfaced so the
/// setup wizard and dashboard can show live progress without instrumenting the
/// ACME round-trip. Persisted in `system_config`, so it survives a restart.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TlsProvisioningPhase {
    /// No issuance has been attempted (DDNS unconfigured or freshly installed).
    Idle,
    /// Issuance is in flight (publishing the ACME challenge / awaiting the CA).
    Issuing,
    /// A certificate is live for the active domain.
    Issued,
    /// The last issuance attempt failed; see [`TlsStatusResponse::error`].
    Failed,
}

/// Response for `GET /api/tls/status`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TlsStatusResponse {
    /// Current coarse provisioning phase.
    pub phase: TlsProvisioningPhase,
    /// The domain being (or already) provisioned, if any.
    pub domain: Option<String>,
    /// RFC 3339 expiry of the stored certificate, when one has been issued.
    pub not_after: Option<String>,
    /// Human-readable error from the last failed attempt, when `phase` is
    /// [`TlsProvisioningPhase::Failed`].
    pub error: Option<String>,
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
    // `required` keeps the key in the schema's `required` list: serde always
    // serializes this field (as `null` when unset), so the accurate contract
    // is required-but-nullable, not optional.
    #[schema(required, value_type = Option<String>)]
    pub gateway: Option<std::net::Ipv4Addr>,
    pub dhcp_source: DhcpSource,
    /// Upstream router MAC persisted by the wizard's discover step, if
    /// any — lets the review step display it without re-probing.
    pub router_mac: Option<String>,
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
    #[schema(value_type = Option<String>)]
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
    // Required-but-nullable, same reasoning as `NetworkStatusResponse::gateway`.
    #[schema(required, value_type = Option<String>)]
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

/// Request body for POST /api/dhcp/config/preview.
///
/// A dry-run that reports which active leases would be revoked if the pool
/// were changed to `pool_start`–`pool_end`, so the admin can be warned before
/// saving. Mutates nothing.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PreviewDhcpConfigRequest {
    pub pool_start: String,
    pub pool_end: String,
}

/// Response for POST /api/dhcp/config/preview.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PreviewDhcpConfigResponse {
    /// Active leases whose IP would fall outside the proposed pool and are not
    /// pinned by a reservation. These devices would be forced to re-acquire an
    /// in-range lease on their next renewal.
    pub affected: Vec<DhcpLease>,
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
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct UpdateDnsConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_mode: Option<DnsResolutionMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_servers: Option<Vec<UpstreamDnsRequest>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forwarder_selection_mode: Option<ForwarderSelectionMode>,
    /// The chosen single upstream's `address`. Only consulted when the
    /// resulting mode is `single`; ignored (and forced clear) in the other
    /// modes. Omit to leave the current selection unchanged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub single_upstream: Option<String>,
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
    /// Per-upstream answer deadline on the forwarding ladder, in
    /// milliseconds. Must not exceed `forward_deadline_ms`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_timeout_ms: Option<u32>,
    /// Wall-clock ceiling on a whole forwarded query, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forward_deadline_ms: Option<u32>,
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
    /// TLS server name (SNI) — required when protocol is tls/https.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_server_name: Option<String>,
}

impl From<UpstreamDnsRequest> for UpstreamDns {
    fn from(req: UpstreamDnsRequest) -> Self {
        Self {
            address: req.address,
            name: req.name,
            protocol: req.protocol,
            port: req.port,
            tls_server_name: req.tls_server_name,
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
    /// Rolling-average latency per configured upstream, from the background
    /// prober. One entry per `upstream_servers` address; empty until the
    /// prober has run at least once.
    pub upstream_latencies: Vec<UpstreamLatency>,
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
    /// Optional free-text annotation (max 200 characters).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Response for POST /api/dns/filter/profiles.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateProfileResponse {
    pub profile: DnsFilterProfile,
    pub message: String,
}

/// Request body for PUT /api/dns/filter/profiles/{id}.
///
/// All fields are partial-update: omitting them from JSON leaves the stored
/// value unchanged. Pass `description: null` to clear an existing description.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateProfileRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `undefined` / omitted = leave unchanged. `null` = clear. `string` = set.
    ///
    /// Standard serde cannot distinguish "field absent" from "field: null" for
    /// `Option<T>`. The custom deserializer below wraps the inner deserialize so
    /// that `null` → `Some(None)` and absent → `None`, giving us three states.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::serde_util::nullable_field"
    )]
    pub description: Option<Option<String>>,
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

    /// Whether the filters are actually being enforced right now.
    ///
    /// The DNS server answers queries as soon as it binds, but the compiled
    /// filters are built in the background — and after an upgrade that resets
    /// the blocklist cache, the lists have to be re-downloaded before they can
    /// block anything. During either window DNS resolves normally and
    /// blocklists are **not** enforced, which is a fail-open the admin has to
    /// be able to see. `false` means exactly that.
    pub filters_ready: bool,

    /// How many enabled blocklists have never been imported, i.e. are empty and
    /// enforcing nothing until their first download completes.
    pub pending_blocklists: u32,
}

/// Request body for PUT /api/dns/filter/config.
///
/// Both fields are partial-update: omitted from JSON = no change. For
/// `default_profile_ids`, an empty list clears the default (unassigned
/// devices stop being filtered) and a non-empty list replaces the entire
/// set.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateDnsFilterConfigRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile_ids: Option<Vec<Uuid>>,
    /// Consecutive failed refreshes before a blocklist alerts. `0` disables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocklist_failure_alert_threshold: Option<u32>,
}

// ---------------------------------------------------------------------------
// Anomaly API types
// ---------------------------------------------------------------------------

/// An anomaly as served by the HTTP API.
///
/// `severity`, `component` and `hint` are derived from `anomaly_type` rather
/// than stored, so they are materialised here for clients that would otherwise
/// need their own copy of the catalogue.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ApiAnomaly {
    pub id: Uuid,
    /// Stable machine-readable class identifier, e.g. `tunnel_start_failed`.
    #[serde(rename = "type")]
    pub anomaly_type: AnomalyType,
    /// One of `error`, `warning`, `info`.
    pub severity: AnomalySeverity,
    /// The subsystem that raised it.
    pub component: String,
    /// The entity this anomaly is about — a tunnel id, a blocklist id — or
    /// `null` for box-wide conditions.
    #[schema(required)]
    pub subject_id: Option<String>,
    /// Plain-language description of what happened.
    pub message: String,
    /// What the admin can do about it.
    pub hint: String,
    /// Detector-authored payload. Best-effort: no key is guaranteed beyond
    /// those the owning detector documents. Clients use it to deep link to the
    /// subject (a blocklist's `profile_id`, for instance).
    #[schema(required, value_type = Option<Object>)]
    pub details: Option<serde_json::Value>,
    pub opened_at: DateTime<Utc>,
    /// When the condition was last observed.
    pub last_seen_at: DateTime<Utc>,
    /// How many times it has been observed since it opened.
    pub occurrences: u32,
    /// `null` while the anomaly is still open.
    #[schema(required)]
    pub resolved_at: Option<DateTime<Utc>>,
}

impl From<Anomaly> for ApiAnomaly {
    fn from(a: Anomaly) -> Self {
        Self {
            id: a.id,
            severity: a.anomaly_type.severity(),
            component: a.anomaly_type.component().to_owned(),
            hint: a.anomaly_type.hint().to_owned(),
            anomaly_type: a.anomaly_type,
            subject_id: a.subject_id,
            message: a.message,
            details: a.details,
            opened_at: a.opened_at,
            last_seen_at: a.last_seen_at,
            occurrences: a.occurrences,
            resolved_at: a.resolved_at,
        }
    }
}

/// Response for GET /api/anomalies.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListAnomaliesResponse {
    pub anomalies: Vec<ApiAnomaly>,
}

/// Query parameters for GET /api/anomalies.
#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams)]
pub struct ListAnomaliesParams {
    /// Which anomalies to return. Defaults to `open`.
    pub status: Option<AnomalyQueryStatus>,
    /// Restrict to anomalies about one entity — how a per-entity surface asks
    /// "what is currently wrong with *this*".
    pub subject_id: Option<String>,
    /// Maximum rows, clamped to 1..=200. Defaults to 50.
    pub limit: Option<u32>,
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
    /// Filter by the device attributed at query time. Stable across DHCP
    /// reassignment; rows recorded before device attribution existed (or
    /// from unknown sources) have no device and only match `client_ip`.
    #[serde(default)]
    pub device_id: Option<Uuid>,
    #[serde(default)]
    pub result: Option<DnsQueryResult>,
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
    pub result: DnsQueryResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    pub latency_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// Request body for `PATCH /api/devices/{id}/dns-capture` — the admin capture
/// controls. Omitted fields are left unchanged (merged via SQL `COALESCE`).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema, Default)]
pub struct DnsCaptureSettingsRequest {
    pub enabled: Option<bool>,
    pub cap_count: Option<i64>,
    pub cap_days: Option<i64>,
}

/// Response for `GET`/`PATCH /api/devices/{id}/dns-capture` — current capture
/// settings alongside storage stats for the device.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsCaptureSettingsResponse {
    pub enabled: bool,
    pub cap_count: i64,
    pub cap_days: i64,
    pub row_count: i64,
    pub size_bytes: i64,
}

/// Request body for `PATCH /api/devices/me/dns-capture` — the self-service
/// capture toggle. Flips only the `enabled` flag; retention caps stay
/// admin-only (set via `DnsCaptureSettingsRequest` on the admin endpoint).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeviceCaptureToggleRequest {
    pub enabled: bool,
}

// ===========================================================================
// Local DNS — authoritative zones, custom records, forwarding rules (issue
// #217). CRUD surface under /api/dns/local/..., parallel to /api/dns/filter.
// Resolver lookup and upsert are deferred to later chunks; these are plain
// CRUD DTOs over the C1 `DnsLocalRepository`.
// ===========================================================================

// --- Zones -----------------------------------------------------------------

/// Response for GET /api/dns/local/zones.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListZonesResponse {
    pub zones: Vec<DnsZone>,
}

/// Response for GET /api/dns/local/zones/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GetZoneResponse {
    pub zone: DnsZone,
}

/// Request body for POST /api/dns/local/zones.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateZoneRequest {
    pub name: String,
    pub enabled: bool,
}

/// Response for POST /api/dns/local/zones.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateZoneResponse {
    pub zone: DnsZone,
    pub message: String,
}

/// Request body for PUT /api/dns/local/zones/{id} (partial update).
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateZoneRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Response for PUT /api/dns/local/zones/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateZoneResponse {
    pub zone: DnsZone,
    pub message: String,
}

/// Response for DELETE /api/dns/local/zones/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteZoneResponse {
    pub message: String,
}

// --- Records ---------------------------------------------------------------

/// Response for GET /api/dns/local/records and
/// GET /api/dns/local/zones/{id}/records.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListRecordsResponse {
    pub records: Vec<CustomDnsRecord>,
}

/// Response for GET /api/dns/local/records/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GetRecordResponse {
    pub record: CustomDnsRecord,
}

/// Request body for POST /api/dns/local/records.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateRecordRequest {
    /// Optional owning zone. `null`/omitted creates an unzoned record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone_id: Option<Uuid>,
    pub domain: String,
    pub record_type: DnsRecordType,
    pub value: String,
    pub ttl: u32,
    pub enabled: bool,
}

/// Request body for PUT /api/dns/local/records/{id} (partial update).
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateRecordRequest {
    /// Three-state: omitted = leave zone link unchanged, `null` = clear to
    /// NULL (unzoned), value = reassign. See [`crate::serde_util::nullable_field`].
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::serde_util::nullable_field"
    )]
    #[schema(value_type = Option<Uuid>)]
    pub zone_id: Option<Option<Uuid>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_type: Option<DnsRecordType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Response for POST /api/dns/local/records.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateRecordResponse {
    pub record: CustomDnsRecord,
    pub message: String,
}

/// Response for PUT /api/dns/local/records/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateRecordResponse {
    pub record: CustomDnsRecord,
    pub message: String,
}

/// Response for DELETE /api/dns/local/records/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteRecordResponse {
    pub message: String,
}

/// Insert-or-update request for a custom record keyed on
/// `(domain, record_type)`. Used internally by the DHCP `.lan`
/// auto-registration runner — not exposed as an HTTP endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpsertRecordRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zone_id: Option<Uuid>,
    pub domain: String,
    pub record_type: DnsRecordType,
    pub value: String,
    pub ttl: u32,
    pub enabled: bool,
    pub source: DnsRecordSource,
}

/// Response for an upsert of a custom record.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpsertRecordResponse {
    pub record: CustomDnsRecord,
    pub message: String,
}

// --- Forwarding rules ------------------------------------------------------

/// Response for GET /api/dns/local/forwarding.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListForwardingRulesResponse {
    pub rules: Vec<ConditionalForwardingRule>,
}

/// Response for GET /api/dns/local/forwarding/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GetForwardingRuleResponse {
    pub rule: ConditionalForwardingRule,
}

/// Request body for POST /api/dns/local/forwarding.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateForwardingRuleRequest {
    pub domain: String,
    pub upstream: String,
    pub enabled: bool,
}

/// Request body for PUT /api/dns/local/forwarding/{id} (partial update).
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateForwardingRuleRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Response for POST /api/dns/local/forwarding.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateForwardingRuleResponse {
    pub rule: ConditionalForwardingRule,
    pub message: String,
}

/// Response for PUT /api/dns/local/forwarding/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateForwardingRuleResponse {
    pub rule: ConditionalForwardingRule,
    pub message: String,
}

/// Response for DELETE /api/dns/local/forwarding/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteForwardingRuleResponse {
    pub message: String,
}

/// A single captured DNS event streamed over
/// `GET /api/devices/me/dns-events/stream` as an SSE message.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsEventItem {
    pub id: i64,
    pub domain: String,
    pub status: String,
    pub captured_at: String,
}

/// Request body for `POST /api/devices/me/dns-events/ack` — acknowledges every
/// captured event up to and including `up_to_id` so the daemon can delete them.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DnsEventsAckRequest {
    pub up_to_id: i64,
}

/// Standard Web Push subscription object as produced in-browser by
/// `PushManager.subscribe()`. Body of `POST /api/push/subscriptions`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WebPushSubscription {
    /// Push service endpoint the daemon POSTs encrypted payloads to.
    pub endpoint: String,
    pub keys: WebPushKeys,
}

/// The subscriber's encryption keys, carried inside [`WebPushSubscription`].
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WebPushKeys {
    /// Base64url (unpadded) P-256 ECDH public key of the subscriber (`p256dh`).
    pub p256dh: String,
    /// Base64url (unpadded) shared authentication secret (`auth`).
    pub auth: String,
}

/// Response of `GET /api/push/vapid-public-key`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct VapidPublicKeyResponse {
    /// Base64url (unpadded) uncompressed P-256 application server key, passed
    /// to `PushManager.subscribe({ applicationServerKey })` in the browser.
    pub key: String,
}

/// Response of the push subscription mutation endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PushSubscriptionResponse {
    pub message: String,
}

/// One entry of the admin notification feed (issue #482): a persisted record
/// of an admin-audience push notification.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NotificationItem {
    pub id: String,
    /// Stable machine tag, e.g. `new_device_quarantined`, `tunnel_offline`,
    /// `routing_changed`.
    pub kind: String,
    pub title: String,
    pub body: String,
    /// App-relative deep link (no PWA base path), e.g. `/devices`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Identifier of the subject entity; what it identifies is driven by
    /// `kind` (device UUID for device kinds, tunnel UUID for tunnel kinds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}

/// Response of `GET /api/push/notifications`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NotificationsResponse {
    /// Newest first.
    pub notifications: Vec<NotificationItem>,
}

// --- Network Zones (epic #244, issue #735) ---------------------------------

/// A Network Zone plus its current member count. Enriched view returned by the
/// zone list/detail endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NetworkZoneView {
    #[serde(flatten)]
    pub zone: NetworkZone,
    /// Number of devices currently assigned to this zone.
    pub member_count: i64,
}

/// Response for GET /api/network/zones.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListNetworkZonesResponse {
    pub zones: Vec<NetworkZoneView>,
}

/// Response for GET /api/network/zones/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GetNetworkZoneResponse {
    pub zone: NetworkZoneView,
}

/// Request body for POST /api/network/zones. `provenance` is forced to `Manual`
/// server-side; default flags are never set on create (use PUT to promote).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateNetworkZoneRequest {
    pub name: String,
    pub isolation_stance: ZoneStance,
    pub allowed_targets: Vec<AllowedTargetKind>,
    pub member_isolation: bool,
    pub admin_ui_reachable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subnet: Option<ZoneSubnet>,
}

/// Response for POST /api/network/zones.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateNetworkZoneResponse {
    pub zone: NetworkZone,
}

/// Request body for PUT /api/network/zones/{id} (partial update). Every field
/// is optional. `is_default`/`is_default_for_new` set to `Some(true)` promote
/// this zone to the anchor / default-for-new; `Some(false)` is rejected (flags
/// only move by promoting another zone). `subnet: Some(None)` clears the subnet.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateNetworkZoneRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation_stance: Option<ZoneStance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_targets: Option<Vec<AllowedTargetKind>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_isolation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin_ui_reachable: Option<bool>,
    // Three-state: absent → `None` (leave as-is); `null` → `Some(None)` (clear);
    // a value → `Some(Some(..))` (set). Needs `nullable_field` because plain
    // serde maps JSON `null` on `Option<Option<T>>` to the outer `None`.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::serde_util::nullable_field"
    )]
    pub subnet: Option<Option<ZoneSubnet>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default_for_new: Option<bool>,
}

/// Response for PUT /api/network/zones/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateNetworkZoneResponse {
    pub zone: NetworkZone,
}

/// Response for DELETE /api/network/zones/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteNetworkZoneResponse {
    pub deleted: bool,
}

/// Request body for PUT /api/devices/{id}/zone.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AssignDeviceZoneRequest {
    pub zone_id: Uuid,
}

/// Response for GET /api/network/quarantine-new-devices (issue #738).
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct QuarantineNewDevicesResponse {
    /// Whether new-device quarantine is on. Off by default.
    pub enabled: bool,
}

/// Request body for PUT /api/network/quarantine-new-devices (issue #738).
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetQuarantineNewDevicesRequest {
    pub enabled: bool,
}

// --- Zone Exceptions (epic #244, issue #737) -------------------------------

/// Response for GET /api/network/zones/exceptions.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListZoneExceptionsResponse {
    pub exceptions: Vec<ZoneException>,
}

/// Response for GET /api/network/zones/exceptions/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GetZoneExceptionResponse {
    pub exception: ZoneException,
}

/// Request body for POST /api/network/zones/exceptions.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateZoneExceptionRequest {
    pub from: ExceptionEndpoint,
    pub to: ExceptionEndpoint,
    pub service: ServiceSpec,
    pub bidirectional: bool,
}

/// Response for POST /api/network/zones/exceptions.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateZoneExceptionResponse {
    pub exception: ZoneException,
}

/// Request body for PUT /api/network/zones/exceptions/{id} (partial update).
/// Every field is optional; absent fields are left unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateZoneExceptionRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<ExceptionEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<ExceptionEndpoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<ServiceSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bidirectional: Option<bool>,
}

/// Response for PUT /api/network/zones/exceptions/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateZoneExceptionResponse {
    pub exception: ZoneException,
}

/// Response for DELETE /api/network/zones/exceptions/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteZoneExceptionResponse {
    pub deleted: bool,
}

// --- Routing profiles (issue #241) -----------------------------------------

/// Response for GET /api/routing/profiles.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListRoutingProfilesResponse {
    pub profiles: Vec<RoutingProfile>,
}

/// Response for GET /api/routing/profiles/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GetRoutingProfileResponse {
    pub profile: RoutingProfile,
}

/// Request body for POST /api/routing/profiles.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateRoutingProfileRequest {
    pub name: String,
}

/// Response for POST /api/routing/profiles.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateRoutingProfileResponse {
    pub profile: RoutingProfile,
    pub message: String,
}

/// Request body for PUT /api/routing/profiles/{id}.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateRoutingProfileRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Response for PUT /api/routing/profiles/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateRoutingProfileResponse {
    pub profile: RoutingProfile,
    pub message: String,
}

/// Response for DELETE /api/routing/profiles/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteRoutingProfileResponse {
    pub message: String,
}

/// Response for GET /api/routing/profiles/{id}/rules.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListDomainRoutingRulesResponse {
    pub rules: Vec<DomainRoutingRule>,
}

/// Request body for POST /api/routing/profiles/{id}/rules.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateDomainRoutingRuleRequest {
    pub pattern: String,
    pub target: DomainRoutingTarget,
    pub enabled: bool,
}

/// Response for POST /api/routing/profiles/{id}/rules.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateDomainRoutingRuleResponse {
    pub rule: DomainRoutingRule,
    pub message: String,
}

/// Request body for PUT /api/routing/rules/{id} (partial update).
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateDomainRoutingRuleRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<DomainRoutingTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Response for PUT /api/routing/rules/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateDomainRoutingRuleResponse {
    pub rule: DomainRoutingRule,
    pub message: String,
}

/// Response for DELETE /api/routing/rules/{id}.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeleteDomainRoutingRuleResponse {
    pub message: String,
}

/// Response for GET /`api/routing/devices/{device_id}/profiles`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GetDeviceRoutingProfilesResponse {
    /// Assigned profile ids, in priority order (first = highest priority).
    pub profile_ids: Vec<Uuid>,
}

/// Request body for PUT /`api/routing/devices/{device_id}/profiles`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetDeviceRoutingProfilesRequest {
    /// Profile ids in priority order (first = highest priority).
    pub profile_ids: Vec<Uuid>,
}

/// Response for PUT /`api/routing/devices/{device_id}/profiles`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetDeviceRoutingProfilesResponse {
    pub message: String,
}

/// Response for GET /`api/routing/profiles/{id}/devices`.
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ListProfileDevicesResponse {
    /// Device ids the profile is currently assigned to.
    pub device_ids: Vec<Uuid>,
}
