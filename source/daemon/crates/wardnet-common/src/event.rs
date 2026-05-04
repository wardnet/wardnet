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
    DnsBlocklistUpdated {
        blocklist_id: Uuid,
        entry_count: u64,
        timestamp: DateTime<Utc>,
    },
    RouteTableLost {
        table: u32,
        timestamp: DateTime<Utc>,
    },
    /// Filter inputs (blocklists, allowlist, custom rules) changed via CRUD or
    /// blocklist refresh — runner should rebuild the in-memory `DnsFilter`.
    DnsFiltersChanged {
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
}
