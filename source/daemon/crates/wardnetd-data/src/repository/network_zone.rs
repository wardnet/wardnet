use async_trait::async_trait;
use wardnet_common::network_zone::NetworkZone;

/// Data access for Network Zones (epic #244, issue #735).
///
/// Zones are policy buckets a device belongs to (exactly one). All business
/// logic (system-zone / default guards, uniqueness) lives in
/// [`NetworkZoneService`](crate::service::NetworkZoneService); this trait is
/// pure persistence.
#[async_trait]
pub trait NetworkZoneRepository: Send + Sync {
    /// Return all zones, ordered by name.
    async fn find_all(&self) -> anyhow::Result<Vec<NetworkZone>>;

    /// Find a zone by its primary key.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<NetworkZone>>;

    /// Return the anchor zone (`is_default = 1`).
    async fn find_default(&self) -> anyhow::Result<NetworkZone>;

    /// Return the zone freshly-discovered devices are assigned to
    /// (`is_default_for_new = 1`).
    async fn find_default_for_new(&self) -> anyhow::Result<NetworkZone>;

    /// Insert a new zone.
    async fn insert(&self, zone: &NetworkZone) -> anyhow::Result<()>;

    /// Update an existing zone by id (all mutable columns).
    async fn update(&self, zone: &NetworkZone) -> anyhow::Result<()>;

    /// Delete a zone by id. The caller guards against deleting system/default
    /// zones or zones with members; the FK `ON DELETE RESTRICT` is the
    /// belt-and-suspenders backstop.
    async fn delete(&self, id: &str) -> anyhow::Result<()>;

    /// Transactionally move the anchor (`is_default`) flag to the given zone.
    async fn set_default(&self, id: &str) -> anyhow::Result<()>;

    /// Transactionally move the default-for-new (`is_default_for_new`) flag to
    /// the given zone.
    async fn set_default_for_new(&self, id: &str) -> anyhow::Result<()>;

    /// Count the devices currently assigned to the given zone.
    async fn count_members(&self, zone_id: &str) -> anyhow::Result<i64>;
}
