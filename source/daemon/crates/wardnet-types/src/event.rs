use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::routing::{RoutingTarget, RuleCreator};
use crate::tunnel::TunnelStatus;

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
    TunnelUp {
        tunnel_id: Uuid,
        interface_name: String,
        endpoint: String,
        timestamp: DateTime<Utc>,
    },
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
}
