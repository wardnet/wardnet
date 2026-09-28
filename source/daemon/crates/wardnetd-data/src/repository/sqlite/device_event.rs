use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use sqlx::{Row, SqlitePool};

use crate::DbPools;
use wardnet_common::device_event::{DeviceEvent, DeviceEventKind};

use crate::repository::device_event::{DeviceEventRepository, NewDeviceEvent};

pub struct SqliteDeviceEventRepository {
    pools: DbPools,
}

impl SqliteDeviceEventRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    /// Create with split reader/writer pools.
    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl DeviceEventRepository for SqliteDeviceEventRepository {
    async fn record(&self, event: NewDeviceEvent<'_>) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO device_events (device_id, mac, kind, details, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(event.device_id)
        .bind(event.mac)
        .bind(event.kind.as_str())
        .bind(event.details)
        .bind(event.created_at.timestamp())
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn list_for_device(
        &self,
        device_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> anyhow::Result<Vec<DeviceEvent>> {
        // `id` breaks ties so two events in the same second keep insertion
        // order; without it the timeline could reorder them between reads.
        let rows = sqlx::query(
            "SELECT id, device_id, mac, kind, details, created_at \
             FROM device_events \
             WHERE device_id = ? AND created_at >= ? AND created_at <= ? \
             ORDER BY created_at, id",
        )
        .bind(device_id)
        .bind(from.timestamp())
        .bind(to.timestamp())
        .fetch_all(&self.pools.read)
        .await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let kind: String = row.get("kind");
                // An unrecognised slug is a row from a newer daemon seen after a
                // downgrade. Skip it: one unreadable event must not fail the
                // whole timeline.
                let kind = DeviceEventKind::from_slug(&kind)?;
                let created_at: i64 = row.get("created_at");
                Some(DeviceEvent {
                    id: row.get("id"),
                    device_id: row.get("device_id"),
                    mac: row.get("mac"),
                    kind,
                    details: row.get("details"),
                    created_at: Utc.timestamp_opt(created_at, 0).single()?,
                })
            })
            .collect())
    }

    async fn count_for_mac_since(
        &self,
        mac: &str,
        kind: DeviceEventKind,
        since: DateTime<Utc>,
    ) -> anyhow::Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM device_events \
             WHERE mac = ? AND kind = ? AND created_at >= ?",
        )
        .bind(mac)
        .bind(kind.as_str())
        .bind(since.timestamp())
        .fetch_one(&self.pools.read)
        .await?;
        Ok(count)
    }

    async fn count_by_mac_since(
        &self,
        kind: DeviceEventKind,
        since: DateTime<Utc>,
    ) -> anyhow::Result<Vec<(String, i64)>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT mac, COUNT(*) FROM device_events \
             WHERE kind = ? AND created_at >= ? \
             GROUP BY mac",
        )
        .bind(kind.as_str())
        .bind(since.timestamp())
        .fetch_all(&self.pools.read)
        .await?;
        Ok(rows)
    }

    async fn latest_kind_for_device(
        &self,
        device_id: &str,
    ) -> anyhow::Result<Option<DeviceEventKind>> {
        let kind: Option<String> = sqlx::query_scalar(
            "SELECT kind FROM device_events WHERE device_id = ? \
             ORDER BY created_at DESC, id DESC LIMIT 1",
        )
        .bind(device_id)
        .fetch_optional(&self.pools.read)
        .await?;
        // An unrecognised slug reads as no history, which makes an arrival look
        // like a first sighting — the safe way to be wrong.
        Ok(kind.as_deref().and_then(DeviceEventKind::from_slug))
    }

    async fn prune(&self, older_than: DateTime<Utc>, max_per_device: u32) -> anyhow::Result<u64> {
        let aged = sqlx::query("DELETE FROM device_events WHERE created_at < ?")
            .bind(older_than.timestamp())
            .execute(&self.pools.write)
            .await?
            .rows_affected();

        // The row cap is per device, so a quiet device is never evicted by a
        // noisy one. ROW_NUMBER partitions the ranking; ordering by
        // (created_at, id) DESC keeps the newest and breaks same-second ties
        // the same way the read path does.
        let capped = sqlx::query(
            "DELETE FROM device_events WHERE id IN ( \
                 SELECT id FROM ( \
                     SELECT id, ROW_NUMBER() OVER ( \
                         PARTITION BY device_id ORDER BY created_at DESC, id DESC \
                     ) AS rn \
                     FROM device_events \
                 ) WHERE rn > ? \
             )",
        )
        .bind(max_per_device)
        .execute(&self.pools.write)
        .await?
        .rows_affected();

        Ok(aged + capped)
    }
}
