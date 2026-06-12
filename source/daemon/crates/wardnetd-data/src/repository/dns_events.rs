use async_trait::async_trait;

/// Row counts and approximate storage for a device's captured DNS events.
#[derive(Debug, Clone)]
pub struct DnsCaptureStats {
    pub row_count: i64,
    pub size_bytes: i64,
}

#[async_trait]
pub trait DnsEventsRepository: Send + Sync {
    /// Insert a single captured DNS event.
    async fn insert(
        &self,
        device_id: &str,
        domain: &str,
        status: &str,
        captured_at: &str,
    ) -> anyhow::Result<()>;

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
}
