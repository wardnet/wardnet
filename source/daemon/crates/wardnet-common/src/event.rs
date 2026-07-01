use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::routing::{RoutingTarget, RuleCreator};
use crate::tunnel::TunnelStatus;
use crate::update::InstallPhase;

/// Domain events emitted by the Wardnet daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WardnetEvent {
    DeviceDiscovered {
        device_id: Uuid,
        mac: String,
        ip: String,
        hostname: Option<String>,
        timestamp: DateTime<Utc>,
    },
    DeviceIpChanged {
        device_id: Uuid,
        mac: String,
        old_ip: String,
        new_ip: String,
        timestamp: DateTime<Utc>,
    },
    DeviceGone {
        device_id: Uuid,
        mac: String,
        last_ip: String,
        timestamp: DateTime<Utc>,
    },
    /// Bring-up succeeded; kernel interface is configured. No handshake has
    /// been observed yet — the tunnel is awaiting its first peer reply.
    TunnelConnecting {
        tunnel_id: Uuid,
        interface_name: String,
        endpoint: String,
        timestamp: DateTime<Utc>,
    },
    /// First handshake observed for a `Connecting` tunnel, or recovery from
    /// `Reconnecting` after the peer started replying again.
    TunnelUp {
        tunnel_id: Uuid,
        interface_name: String,
        endpoint: String,
        timestamp: DateTime<Utc>,
    },
    /// Previously `Up` tunnel has not seen a handshake in over 3 minutes.
    /// The kernel interface remains configured; `WireGuard` keepalive is
    /// retrying. Status will flip back to `Up` automatically when the peer
    /// replies, or the operator can intervene.
    TunnelReconnecting {
        tunnel_id: Uuid,
        interface_name: String,
        last_handshake: Option<DateTime<Utc>>,
        timestamp: DateTime<Utc>,
    },
    /// Kernel interface torn down by user/admin action (manual tear-down or
    /// tunnel deletion).
    TunnelDown {
        tunnel_id: Uuid,
        interface_name: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    TunnelStatsUpdated {
        tunnel_id: Uuid,
        status: TunnelStatus,
        bytes_tx: u64,
        bytes_rx: u64,
        last_handshake: Option<DateTime<Utc>>,
        timestamp: DateTime<Utc>,
    },
    RoutingRuleChanged {
        device_id: Uuid,
        target: RoutingTarget,
        previous_target: Option<RoutingTarget>,
        changed_by: RuleCreator,
        timestamp: DateTime<Utc>,
    },
    DhcpLeaseAssigned {
        lease_id: Uuid,
        mac: String,
        ip: String,
        hostname: Option<String>,
        timestamp: DateTime<Utc>,
    },
    DhcpLeaseRenewed {
        lease_id: Uuid,
        mac: String,
        ip: String,
        hostname: Option<String>,
        new_expiry: DateTime<Utc>,
        timestamp: DateTime<Utc>,
    },
    DhcpLeaseExpired {
        lease_id: Uuid,
        mac: String,
        ip: String,
        timestamp: DateTime<Utc>,
    },
    DhcpConflictDetected {
        mac: String,
        ip: String,
        details: String,
        timestamp: DateTime<Utc>,
    },
    DnsServerStarted {
        timestamp: DateTime<Utc>,
    },
    DnsServerStopped {
        timestamp: DateTime<Utc>,
    },
    DnsConfigChanged {
        timestamp: DateTime<Utc>,
    },
    /// A blocklist's domain set changed (re-download or manual replace).
    /// Listeners rebuild only the matching `DnsFilter`.
    DnsFilterBlocklistUpdated {
        blocklist_id: Uuid,
        entry_count: u64,
        timestamp: DateTime<Utc>,
    },
    RouteTableLost {
        table: u32,
        timestamp: DateTime<Utc>,
    },
    /// DNS filtering state changed. The umbrella event for profile content,
    /// profile membership, device assignment, default-profile changes, and
    /// the global emergency stop. The `change` payload tells the listener
    /// which slice of state to rebuild.
    DnsFilterChanged {
        change: DnsFilterChange,
        timestamp: DateTime<Utc>,
    },
    /// A DNS filter source was rebuilt and the new compiled filter is now
    /// live (i.e. the `ArcSwap` swap has happened). Emitted by
    /// `DnsFilterServiceImpl` after each mutating rebuild. Subscribers
    /// such as `UdpDnsServer` use this to invalidate caches that were
    /// populated against the previous filter — without invalidation,
    /// already-forwarded answers stay served until cache TTL expiry,
    /// so newly-blocked domains keep returning their cached `Pass`
    /// responses (issue #341). Payload is intentionally just
    /// `timestamp`: the flush is coarse (whole cache) so the
    /// subscriber doesn't need to know which filter changed.
    DnsFilterRebuilt {
        timestamp: DateTime<Utc>,
    },
    /// A local DNS zone, custom record, or forwarding rule was created,
    /// updated, or deleted. `domain` is set for record and rule mutations
    /// (the specific domain that changed); `None` for zone mutations
    /// (which require a view rebuild but no per-domain cache eviction).
    DnsLocalChanged {
        domain: Option<String>,
        timestamp: DateTime<Utc>,
    },
    UpdateAvailable {
        current_version: String,
        latest_version: String,
        timestamp: DateTime<Utc>,
    },
    UpdateProgress {
        target_version: String,
        phase: InstallPhase,
        timestamp: DateTime<Utc>,
    },
    UpdateCompleted {
        from_version: String,
        to_version: String,
        timestamp: DateTime<Utc>,
    },
    UpdateFailed {
        target_version: String,
        phase: InstallPhase,
        error: String,
        timestamp: DateTime<Utc>,
    },
    /// A tunnel's `override_default_dns` flag was toggled. Listeners
    /// rebuild the routing service's `device_ip → UpstreamId` snapshot
    /// so already-applied device rules pick up the new upstream choice
    /// without waiting for the next routing-rule mutation.
    TunnelDnsOverrideChanged {
        tunnel_id: Uuid,
        timestamp: DateTime<Utc>,
    },
    DeviceCaptureSettingsChanged {
        device_id: Uuid,
        enabled: bool,
        timestamp: DateTime<Utc>,
    },
    /// A DNS capture event was durably inserted for a capture-enabled device.
    /// Emitted by `capture_runner` after the DB write succeeds so the SSE
    /// stream handler can push the row to the device without polling.
    DnsEventInserted {
        device_id: Uuid,
        row_id: i64,
        domain: String,
        status: String,
        captured_at: String,
        timestamp: DateTime<Utc>,
    },
    /// A Network Zone was created, updated, deleted, or had a default flag
    /// moved. Phase 1 (issue #735) has no consumer; the CI-2/CI-3 enforcers
    /// (issues #736/#737) subscribe to rebuild their nftables/subnet state.
    NetworkZoneChanged {
        zone_id: Uuid,
        timestamp: DateTime<Utc>,
    },
    /// A device was reassigned from one Network Zone to another. Phase 1 has no
    /// consumer; later epic children rebuild per-device enforcement.
    DeviceZoneChanged {
        device_id: Uuid,
        old_zone_id: Uuid,
        new_zone_id: Uuid,
        timestamp: DateTime<Utc>,
    },
}

/// What kind of DNS filtering change happened. Carried by
/// [`WardnetEvent::DnsFilterChanged`] so listeners can rebuild only the
/// affected slice of the in-memory filter pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DnsFilterChange {
    /// A profile's filter sources (blocklists, allowlist, rules) changed.
    /// Rebuild only that profile's compiled `DnsFilterProfile`.
    ProfileContent { profile_id: Uuid },
    /// Profile metadata changed (e.g. renamed, created, deleted).
    /// Listeners may need to refresh their profile-id lookup.
    ProfileMembership { profile_id: Uuid },
    /// A device's profile assignment or `enabled` flag changed.
    /// Rebuild only that device's per-IP cache entry.
    DeviceAssignment { device_id: Uuid },
    /// The global default profile pointer changed.
    /// Rebuild the "default context" used for unassigned devices.
    DefaultProfile,
    /// The global emergency-stop flag flipped.
    GlobalToggle,
}
