use std::net::Ipv4Addr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// DHCP pool configuration.
///
/// Wardnet IS the gateway and DNS server — no need for users to configure those.
/// The daemon auto-detects its own LAN IP and advertises itself as gateway (option 3)
/// and DNS server (option 6) to clients. Option 3 includes the router IP as secondary
/// entry for automatic failover on supported clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpConfig {
    pub enabled: bool,
    pub pool_start: Ipv4Addr,
    pub pool_end: Ipv4Addr,
    pub subnet_mask: Ipv4Addr,
    pub upstream_dns: Vec<Ipv4Addr>,
    pub lease_duration_secs: u32,
    pub router_ip: Option<Ipv4Addr>,
}

/// The current status of a DHCP lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DhcpLeaseStatus {
    Active,
    Expired,
    Released,
}

/// An active or historical DHCP lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DhcpLease {
    pub id: Uuid,
    pub mac_address: String,
    pub ip_address: Ipv4Addr,
    pub hostname: Option<String>,
    pub lease_start: DateTime<Utc>,
    pub lease_end: DateTime<Utc>,
    pub status: DhcpLeaseStatus,
    pub device_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A static MAC-to-IP reservation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DhcpReservation {
    pub id: Uuid,
    pub mac_address: String,
    pub ip_address: Ipv4Addr,
    pub hostname: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A DHCP lease audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpLeaseLog {
    pub id: i64,
    pub lease_id: Uuid,
    pub event_type: DhcpLeaseEventType,
    pub details: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Types of events that can occur for a DHCP lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DhcpLeaseEventType {
    Assigned,
    Renewed,
    Released,
    Expired,
    Conflict,
}
