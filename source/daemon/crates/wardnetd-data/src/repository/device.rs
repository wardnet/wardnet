use async_trait::async_trait;
use wardnet_common::device::{Device, DeviceConnectionMode, ManufacturerSource};
use wardnet_common::routing::RoutingRule;

/// Row data for inserting a new device.
pub struct DeviceRow {
    pub id: String,
    pub mac: String,
    pub hostname: Option<String>,
    pub manufacturer: Option<String>,
    /// Provenance of `manufacturer`; `None` exactly when `manufacturer` is
    /// `None` (issue #1099).
    pub manufacturer_source: Option<ManufacturerSource>,
    /// Whether `mac` is locally administered (a privacy/randomized address).
    pub is_randomized: bool,
    pub device_type: String,
    pub first_seen: String,
    pub last_seen: String,
    pub last_ip: String,
    /// The Network Zone the device is assigned to at insert time (sticky).
    /// Resolved from the default-for-new zone by the discovery service.
    pub zone_id: String,
    /// How the device was first observed (live status, last-observation-wins).
    /// Freshly discovered devices are always [`DeviceConnectionMode::Lan`].
    pub connection_mode: DeviceConnectionMode,
}

/// Data access for devices and their routing rules.
///
/// Provides lookups by IP and ID, routing rule queries, and upserts.
/// All business logic (e.g. admin-lock checks) belongs in
/// [`DeviceService`](crate::service::DeviceService).
#[async_trait]
pub trait DeviceRepository: Send + Sync {
    /// Find a device by its most recently observed IP address.
    async fn find_by_ip(&self, ip: &str) -> anyhow::Result<Option<Device>>;

    /// Find **every** device whose `last_ip` equals `ip`.
    ///
    /// Unlike [`find_by_ip`](Self::find_by_ip), which resolves a shared address
    /// to the most-recently-seen claimant, this exposes the full set so a caller
    /// can detect an ambiguous mapping. `last_ip` is not unique — a departed
    /// device keeps its row until discovery clears it — so a recycled address
    /// can transiently belong to more than one row. The mDNS observer uses this
    /// to skip attributing a vendor when zero or more than one device claims an
    /// address (issue #1115).
    ///
    /// An empty `ip` (the "no known address" sentinel of departed rows) returns
    /// an empty set, never the departed rows that share it.
    ///
    /// The default implementation filters [`find_all`](Self::find_all); the
    /// `SQLite` implementation overrides it with an indexed `last_ip` lookup.
    async fn find_all_by_ip(&self, ip: &str) -> anyhow::Result<Vec<Device>> {
        if ip.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .find_all()
            .await?
            .into_iter()
            .filter(|d| d.last_ip == ip)
            .collect())
    }

    /// Find a device by its primary key.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>>;

    /// Find a device by its MAC address.
    async fn find_by_mac(&self, mac: &str) -> anyhow::Result<Option<Device>>;

    /// Return all devices.
    async fn find_all(&self) -> anyhow::Result<Vec<Device>>;

    /// Insert a new device record.
    async fn insert(&self, device: &DeviceRow) -> anyhow::Result<()>;

    /// Update `last_seen` timestamp, IP, and connection mode for a device in
    /// one write.
    ///
    /// `mode` records how the device was just observed (LAN vs. inbound
    /// `WireGuard`) — a live, last-observation-wins status, so every
    /// observation path stamps it: LAN discovery passes
    /// [`DeviceConnectionMode::Lan`], the inbound-WireGuard handshake path
    /// passes [`DeviceConnectionMode::Remote`].
    async fn update_last_seen_and_ip(
        &self,
        id: &str,
        ip: &str,
        last_seen: &str,
        mode: DeviceConnectionMode,
    ) -> anyhow::Result<()>;

    /// Clear a device's `last_ip`, leaving the row otherwise intact.
    ///
    /// Called when a device is marked gone so its (now stale) row can no longer
    /// collide with a live device's address in [`find_by_ip`](Self::find_by_ip)
    /// once DHCP recycles the departed device's IP. The address is emptied
    /// rather than the row deleted — device history is retained; the row is just
    /// no longer resolvable by IP until the device is observed again.
    async fn clear_last_ip(&self, id: &str) -> anyhow::Result<()>;

    /// Batch update `last_seen` timestamps. Each tuple is (`device_id`, `last_seen_iso`).
    async fn update_last_seen_batch(&self, updates: &[(String, String)]) -> anyhow::Result<()>;

    /// Update only a device's `connection_mode`, leaving `last_seen` / `last_ip`
    /// untouched.
    ///
    /// Distinct from [`update_last_seen_and_ip`](Self::update_last_seen_and_ip),
    /// which stamps mode *as part of* an observation: this is for correcting a
    /// stale mode without an observation (e.g. resetting a revoked remote
    /// device back to `Lan`). No-op if the device does not exist.
    async fn update_connection_mode(
        &self,
        id: &str,
        mode: DeviceConnectionMode,
    ) -> anyhow::Result<()>;

    /// Update hostname for a device.
    async fn update_hostname(&self, id: &str, hostname: &str) -> anyhow::Result<()>;

    /// Update device name and/or type (admin operation).
    async fn update_name_and_type(
        &self,
        id: &str,
        name: Option<&str>,
        device_type: &str,
    ) -> anyhow::Result<()>;

    /// Find devices whose `last_seen` is older than the given ISO timestamp.
    async fn find_stale(&self, before: &str) -> anyhow::Result<Vec<Device>>;

    /// Return the routing rule for a device, if one exists.
    async fn find_rule_for_device(&self, device_id: &str) -> anyhow::Result<Option<RoutingRule>>;

    /// Return every device's routing rule in a single query.
    ///
    /// Batched companion to [`find_rule_for_device`](Self::find_rule_for_device)
    /// for callers that enrich the whole device list (e.g. `GET /api/devices`)
    /// and must avoid an N+1. There is at most one rule per device, so each
    /// device appears at most once; devices with no rule are simply absent.
    async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>>;

    /// Insert or update a user-created routing rule for a device.
    async fn upsert_user_rule(
        &self,
        device_id: &str,
        target_json: &str,
        now: &str,
    ) -> anyhow::Result<()>;

    /// Return all devices whose routing rule targets the given tunnel.
    async fn find_devices_for_tunnel(&self, tunnel_id: &str) -> anyhow::Result<Vec<Device>>;

    /// Switch all routing rules targeting the given tunnel to `Direct`.
    ///
    /// Returns the device IDs that were updated. Used when a tunnel is deleted
    /// so that affected devices don't lose connectivity.
    async fn switch_tunnel_rules_to_direct(
        &self,
        tunnel_id: &str,
        now: &str,
    ) -> anyhow::Result<Vec<String>>;

    /// Update the `admin_locked` flag for a device.
    async fn update_admin_locked(&self, id: &str, locked: bool) -> anyhow::Result<()>;

    /// Reassign a device to a different Network Zone.
    ///
    /// Returns `true` if a row was updated, `false` if the device was not
    /// found. The `zone_id` FK (`ON DELETE RESTRICT`) rejects unknown zones.
    async fn assign_zone(&self, device_id: &str, zone_id: &str) -> anyhow::Result<bool>;

    /// Return the total number of devices.
    async fn count(&self) -> anyhow::Result<i64>;

    /// Update DNS capture settings for a device.
    ///
    /// Only the `Some` fields are written; `None` leaves the existing value
    /// in place via SQL `COALESCE`. Returns `true` if a row was updated,
    /// `false` if the device was not found.
    async fn update_dns_capture_settings(
        &self,
        id: &str,
        enabled: Option<bool>,
        cap_count: Option<i64>,
        cap_days: Option<i64>,
    ) -> anyhow::Result<bool>;

    /// Return IDs of all devices that have DNS capture enabled.
    async fn find_all_capture_enabled_ids(&self) -> anyhow::Result<Vec<String>>;
}
