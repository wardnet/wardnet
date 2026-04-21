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
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DhcpConfig {
    pub enabled: bool,
    /// Wardnet's own LAN IP — auto-detected from the LAN interface.
    /// Advertised as gateway (option 3) and DNS server (option 6).
    #[schema(value_type = String)]
    pub gateway_ip: Ipv4Addr,
    #[schema(value_type = String)]
    pub pool_start: Ipv4Addr,
    #[schema(value_type = String)]
    pub pool_end: Ipv4Addr,
    #[schema(value_type = String)]
    pub subnet_mask: Ipv4Addr,
    #[schema(value_type = Vec<String>)]
    pub upstream_dns: Vec<Ipv4Addr>,
    pub lease_duration_secs: u32,
    /// Fallback router — the real router's IP, included as secondary in option 3.
    #[schema(value_type = Option<String>)]
    pub router_ip: Option<Ipv4Addr>,
}

/// The current status of a DHCP lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DhcpLeaseStatus {
    Active,
    Expired,
    Released,
}

/// An active or historical DHCP lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DhcpLease {
    pub id: Uuid,
    pub mac_address: String,
    #[schema(value_type = String)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DhcpReservation {
    pub id: Uuid,
    pub mac_address: String,
    #[schema(value_type = String)]
    pub ip_address: Ipv4Addr,
    pub hostname: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A DHCP lease audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DhcpLeaseLog {
    pub id: i64,
    pub lease_id: Uuid,
    pub event_type: DhcpLeaseEventType,
    pub details: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Types of events that can occur for a DHCP lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DhcpLeaseEventType {
    Assigned,
    Renewed,
    Released,
    Expired,
    Conflict,
}
