use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The type/category of a network device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Tv,
    Phone,
    Laptop,
    Tablet,
    GameConsole,
    SettopBox,
    Iot,
    Router,
    ManagedSwitch,
    Server,
    Unknown,
}

/// How a device is currently reachable on the network (issue #810).
///
/// A **live status**, not a lineage tag: a device flips between the two as it
/// connects from different paths over time (last-observation-wins), exactly
/// like `last_ip` already does across DHCP renewals. Set by whichever path
/// most recently observed the device — LAN ARP/DHCP discovery sets [`Lan`], an
/// inbound `WireGuard` handshake sets [`Remote`].
///
/// [`Lan`]: DeviceConnectionMode::Lan
/// [`Remote`]: DeviceConnectionMode::Remote
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceConnectionMode {
    /// Reachable over the LAN (ARP/DHCP). The default for a freshly discovered
    /// device.
    Lan,
    /// Reachable over the inbound `WireGuard` server (#809/#810).
    Remote,
}

/// Whether a device's IP is managed by the wardnet DHCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DhcpStatus {
    /// Device has an active DHCP lease from wardnet.
    Lease,
    /// Device has a static DHCP reservation from wardnet.
    Reservation,
    /// Device IP is not managed by wardnet DHCP (static/external config).
    External,
}

/// A discovered network device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Device {
    pub id: Uuid,
    pub mac: String,
    pub name: Option<String>,
    pub hostname: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: DeviceType,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub last_ip: String,
    pub admin_locked: bool,
    /// The Network Zone this device belongs to (exactly one). Sticky: set from
    /// the default-for-new zone at discovery-insert time; never resolved at read
    /// time. See [`crate::network_zone`] and epic #244.
    pub zone_id: Uuid,
    pub dns_capture_enabled: bool,
    pub dns_capture_cap_count: i64,
    pub dns_capture_cap_days: i64,
    /// How the device is currently reachable (LAN vs. inbound `WireGuard`).
    /// Live status, last-observation-wins — see [`DeviceConnectionMode`].
    pub connection_mode: DeviceConnectionMode,
}
