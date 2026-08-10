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

/// Where a device's manufacturer name came from, which is what licenses the UI
/// to state it as fact or hedge it (issue #1099).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManufacturerSource {
    /// The registrant on record in the IEEE MA-L database. Stated as fact.
    Ieee,
    /// Our own curated vendor-catalog mapping (`wardnetd-data`'s
    /// `vendor_catalog`) for an OUI whose IEEE listing is deliberately
    /// `Private`. Rendered as "likely <vendor>" — we are
    /// asserting something the vendor chose not to publish, and an OUI can be
    /// reassigned out from under us.
    Catalog,
    /// Inferred from something the device announced (mDNS, DHCP vendor class)
    /// or answered (a probed port).
    Signal,
}

/// A single observed fact that helps identify a device (issue #1099).
///
/// Deliberately multi-valued per device: a device can announce several mDNS
/// services, and each signal is independent evidence rather than a field that
/// overwrites the last one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeviceSignal {
    pub kind: DeviceSignalKind,
    pub value: String,
    /// `true` when the value matched a vendor in the curated catalog.
    ///
    /// Deliberately *not* "this signal named the device": naming is
    /// first-writer-wins against a `NULL` manufacturer (see
    /// `set_manufacturer_if_absent`), so a device already named by its IEEE
    /// registrant collects matching signals that changed nothing. Surfaced
    /// because a match is still the evidence an admin needs when the name is
    /// itself a hedge ("Likely Govee").
    pub inferred: bool,
    pub observed_at: DateTime<Utc>,
}

/// The kind of an [`DeviceSignal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceSignalKind {
    /// DHCP option 12 — the hostname the device asked to be known by.
    DhcpHostname,
    /// DHCP option 55 — the parameter-request-list, stored as the raw ordered
    /// code list. The *ordering* is the device-class fingerprint.
    DhcpParamList,
    /// DHCP option 60 — the vendor class identifier, often a literal brand.
    DhcpVendorClass,
    /// An mDNS service type the device advertised (e.g. `_googlecast._tcp`).
    MdnsService,
    /// A TCP port that answered during an admin-triggered identification probe.
    ProbedPort,
}

/// A discovered network device.
// The bools are independent, orthogonal facts about one device — randomized
// MAC, admin lock, DNS capture, managed — not a state machine hiding in a
// struct. Grouping them into sub-structs would change the wire format for no
// gain, since this type is serialized straight onto the API.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Device {
    pub id: Uuid,
    pub mac: String,
    pub name: Option<String>,
    pub hostname: Option<String>,
    pub manufacturer: Option<String>,
    /// Provenance of `manufacturer`. `None` exactly when `manufacturer` is
    /// `None`.
    pub manufacturer_source: Option<ManufacturerSource>,
    /// Whether `mac` is locally administered (a privacy/randomized address).
    /// Deliberately separate from `manufacturer`: it says how the device
    /// presents itself, not who built it.
    pub is_randomized: bool,
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
    /// Whether an admin has decided to control this device's configuration.
    ///
    /// Set by *any* admin configuration act (naming, locking, a routing rule or
    /// profile, DNS filter settings, DNS capture, a Private-DNS grant, a Remote
    /// peer credential, a DHCP reservation, a zone exception, an explicit zone
    /// reassignment) and deliberately **not** inferred from `name` — a device
    /// can be configured without ever being named, and clearing a name must not
    /// silently revoke its remote access.
    ///
    /// Latching: never demotes automatically, only through an explicit release
    /// (`POST /api/devices/{id}/release`), which first reverts every managed
    /// setting to default. That upholds the invariant device retention depends
    /// on — `managed = false` implies no admin artefacts exist — so pruning a
    /// long-absent unmanaged device can never orphan a row. See issue #1181 and
    /// `docs/adr/0032-managed-devices-and-retention.md`.
    pub managed: bool,
}
