use async_trait::async_trait;
use wardnet_types::device::Device;
use wardnet_types::routing::RoutingRule;

/// Row data for inserting a new device.
pub struct DeviceRow {
    pub id: String,
    pub mac: String,
    pub hostname: Option<String>,
    pub manufacturer: Option<String>,
    pub device_type: String,
    pub first_seen: String,
    pub last_seen: String,
    pub last_ip: String,
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

    /// Find a device by its primary key.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>>;

    /// Find a device by its MAC address.
    async fn find_by_mac(&self, mac: &str) -> anyhow::Result<Option<Device>>;

    /// Return all devices.
    async fn find_all(&self) -> anyhow::Result<Vec<Device>>;

    /// Insert a new device record.
    async fn insert(&self, device: &DeviceRow) -> anyhow::Result<()>;

    /// Update `last_seen` timestamp and IP for a device.
    async fn update_last_seen_and_ip(
        &self,
        id: &str,
        ip: &str,
        last_seen: &str,
    ) -> anyhow::Result<()>;

    /// Batch update `last_seen` timestamps. Each tuple is (`device_id`, `last_seen_iso`).
    async fn update_last_seen_batch(&self, updates: &[(String, String)]) -> anyhow::Result<()>;

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

    /// Insert or update a user-created routing rule for a device.
    async fn upsert_user_rule(
        &self,
        device_id: &str,
        target_json: &str,
        now: &str,
    ) -> anyhow::Result<()>;

    /// Update the `admin_locked` flag for a device.
    async fn update_admin_locked(&self, id: &str, locked: bool) -> anyhow::Result<()>;

    /// Return the total number of devices.
    async fn count(&self) -> anyhow::Result<i64>;
}
