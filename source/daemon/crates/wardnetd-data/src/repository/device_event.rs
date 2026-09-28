use async_trait::async_trait;
use chrono::{DateTime, Utc};
use wardnet_common::device_event::{DeviceEvent, DeviceEventKind};

/// How long a device event is retained.
///
/// Matches `DEVICE_RETENTION_DAYS`: an unmanaged device's own row is deleted
/// after 30 days absent, so retaining its events much longer only accumulates
/// rows nothing can be joined back to. Hardcoded rather than configurable,
/// following the precedent ADR 0032 records for device retention.
pub const DEVICE_EVENT_RETENTION_DAYS: i64 = 30;

/// How many events any one device may retain, newest kept.
///
/// The age cap alone gives no ceiling. In steady state a household box writes a
/// few hundred events a day across every device, but a single misbehaving
/// bridge has been observed changing address ~982 times an hour — three orders
/// of magnitude more, and precisely when the timeline is most worth having. The
/// row cap bounds that blast radius while leaving normal operation governed by
/// age alone. 5,000 rows is roughly three days of that pathology: ample to
/// characterise it, far short of letting it fill the disk.
pub const DEVICE_EVENT_MAX_PER_DEVICE: u32 = 5_000;

/// Fields needed to record one device event.
#[derive(Debug, Clone)]
pub struct NewDeviceEvent<'a> {
    pub device_id: &'a str,
    /// Denormalised deliberately — see the migration.
    pub mac: &'a str,
    pub kind: DeviceEventKind,
    /// Serialised JSON, or `None`.
    pub details: Option<&'a str>,
    pub created_at: DateTime<Utc>,
}

/// Persistence for the observational per-device event log (issue #1338).
#[async_trait]
pub trait DeviceEventRepository: Send + Sync {
    /// Append one event. Never deduplicates: two identical observations a
    /// second apart are two facts, and the cadence between them is the signal
    /// the churn detector reads.
    async fn record(&self, event: NewDeviceEvent<'_>) -> anyhow::Result<()>;

    /// Every event for one device inside `[from, to]`, oldest first.
    ///
    /// Rows whose `kind` no longer parses are skipped rather than failing the
    /// query — see [`DeviceEventKind::from_slug`].
    async fn list_for_device(
        &self,
        device_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<DeviceEvent>>;

    /// How many events of `kind` one MAC recorded since `since`.
    ///
    /// Keyed by MAC rather than device id because the address-churn detector
    /// asks a question about a physical station, and the denormalised column
    /// answers it without a join.
    async fn count_for_mac_since(
        &self,
        mac: &str,
        kind: DeviceEventKind,
        since: DateTime<Utc>,
    ) -> anyhow::Result<i64>;

    /// Counts of `kind` per MAC since `since`, for the churn detector's sweep.
    ///
    /// Aggregated in SQL because the detector only needs the counts, and the
    /// station this exists to catch contributes hundreds of rows a day on its
    /// own.
    async fn count_by_mac_since(
        &self,
        kind: DeviceEventKind,
        since: DateTime<Utc>,
    ) -> anyhow::Result<Vec<(String, i64)>>;

    /// The kind of the most recent event recorded for a device, if any.
    ///
    /// Exists because the event bus cannot distinguish a device being seen for
    /// the first time from one returning after an absence: both are published
    /// as `DeviceDiscovered`, and three listeners (routing, zone enforcement,
    /// device snapshot) depend on that, so the bus is deliberately left alone.
    /// The timeline's own history is the tie-breaker — an arrival that follows
    /// a `Gone` is a return.
    async fn latest_kind_for_device(
        &self,
        device_id: &str,
    ) -> anyhow::Result<Option<DeviceEventKind>>;

    /// Apply both retention caps, returning how many rows were deleted.
    async fn prune(&self, older_than: DateTime<Utc>, max_per_device: u32) -> anyhow::Result<u64>;
}
