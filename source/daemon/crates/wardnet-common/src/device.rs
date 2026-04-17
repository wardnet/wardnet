use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The type/category of a network device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Tv,
    Phone,
    Laptop,
    Tablet,
    GameConsole,
    SettopBox,
    Iot,
    Unknown,
}

/// Whether a device's IP is managed by the wardnet DHCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}
