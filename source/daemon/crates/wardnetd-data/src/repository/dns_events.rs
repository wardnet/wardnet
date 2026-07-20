use async_trait::async_trait;

/// Row counts and approximate storage for a device's captured DNS events.
#[derive(Debug, Clone)]
pub struct DnsCaptureStats {
    pub row_count: i64,
    pub size_bytes: i64,
}

/// A single captured DNS event row returned from the repository.
#[derive(Debug, Clone)]
pub struct DnsEventRow {
    pub id: i64,
    pub domain: String,
    pub status: String,
    pub captured_at: String,
}

#[async_trait]
pub trait DnsEventsRepository: Send + Sync {
    /// Insert a single captured DNS event. Returns the auto-increment row ID.
    async fn insert(
        &self,
        device_id: &str,
        domain: &str,
        status: &str,
        captured_at: &str,
    ) -> anyhow::Result<i64>;

    /// Row count and approximate storage for a device's captured events.
    async fn stats_for_device(&self, device_id: &str) -> anyhow::Result<DnsCaptureStats>;

    /// Prune excess events for an enabled device (count-cap + age-cap in one transaction).
    async fn prune_for_device(
        &self,
        device_id: &str,
        cap_count: i64,
        cap_days: i64,
    ) -> anyhow::Result<u64>;

    /// Delete all events for a device (used when capture is disabled).
    async fn delete_all_for_device(&self, device_id: &str) -> anyhow::Result<u64>;

    /// Return the IDs of all devices that have at least one stored event.
    async fn find_device_ids_with_data(&self) -> anyhow::Result<Vec<String>>;

    /// Return pending (unsynced) rows for `device_id` with `id > after_id`,
    /// oldest first, up to `limit` rows.
    async fn fetch_pending(
        &self,
        device_id: &str,
        after_id: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<DnsEventRow>>;

    /// Delete all rows with `id <= up_to_id` for `device_id`.
    /// Returns the number of rows deleted.
    async fn delete_up_to(&self, device_id: &str, up_to_id: i64) -> anyhow::Result<u64>;
}
