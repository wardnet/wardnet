use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::dns_events::{DnsCaptureStats, DnsEventRow, DnsEventsRepository};

#[derive(sqlx::FromRow)]
struct DbDnsEventRow {
    id: i64,
    domain: String,
    status: String,
    captured_at: String,
}

pub struct SqliteDnsEventsRepository {
    pools: DbPools,
}

impl SqliteDnsEventsRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl DnsEventsRepository for SqliteDnsEventsRepository {
    async fn insert(
        &self,
        device_id: &str,
        domain: &str,
        status: &str,
        captured_at: &str,
    ) -> anyhow::Result<i64> {
        let result = sqlx::query(
            "INSERT INTO dns_events (device_id, domain, status, captured_at) VALUES (?, ?, ?, ?)",
        )
        .bind(device_id)
        .bind(domain)
        .bind(status)
        .bind(captured_at)
        .execute(&self.pools.write)
        .await?;
        Ok(result.last_insert_rowid())
    }

    async fn stats_for_device(&self, device_id: &str) -> anyhow::Result<DnsCaptureStats> {
        let row_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM dns_events WHERE device_id = ?")
                .bind(device_id)
                .fetch_one(&self.pools.read)
                .await?;

        let size_bytes = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(\
               LENGTH(domain) + LENGTH(status) + LENGTH(captured_at) + LENGTH(sync_state) + 50\
             ), 0) FROM dns_events WHERE device_id = ?",
        )
        .bind(device_id)
        .fetch_one(&self.pools.read)
        .await?;

        Ok(DnsCaptureStats {
            row_count,
            size_bytes,
        })
    }

    async fn prune_for_device(
        &self,
        device_id: &str,
        cap_count: i64,
        cap_days: i64,
    ) -> anyhow::Result<u64> {
        let mut tx = self.pools.write.begin().await?;

        // Age-based prune: delete events older than cap_days.
        let age_result = sqlx::query(
            "DELETE FROM dns_events WHERE device_id = ? \
             AND captured_at < datetime('now', '-' || ? || ' days')",
        )
        .bind(device_id)
        .bind(cap_days)
        .execute(&mut *tx)
        .await?;

        // Count-based prune: keep only the newest cap_count rows.
        let count_result = sqlx::query(
            "DELETE FROM dns_events WHERE device_id = ? AND id NOT IN \
             (SELECT id FROM dns_events WHERE device_id = ? \
              ORDER BY captured_at DESC LIMIT ?)",
        )
        .bind(device_id)
        .bind(device_id)
        .bind(cap_count)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(age_result.rows_affected() + count_result.rows_affected())
    }

    async fn delete_all_for_device(&self, device_id: &str) -> anyhow::Result<u64> {
        let result = sqlx::query("DELETE FROM dns_events WHERE device_id = ?")
            .bind(device_id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn find_device_ids_with_data(&self) -> anyhow::Result<Vec<String>> {
        let ids = sqlx::query_scalar::<_, String>("SELECT DISTINCT device_id FROM dns_events")
            .fetch_all(&self.pools.read)
            .await?;
        Ok(ids)
    }

    async fn fetch_pending(
        &self,
        device_id: &str,
        after_id: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<DnsEventRow>> {
        let rows = sqlx::query_as::<_, DbDnsEventRow>(
            "SELECT id, domain, status, captured_at \
             FROM dns_events \
             WHERE device_id = ? AND sync_state = 'pending' AND id > ? \
             ORDER BY id ASC LIMIT ?",
        )
        .bind(device_id)
        .bind(after_id)
        .bind(limit)
        .fetch_all(&self.pools.read)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| DnsEventRow {
                id: r.id,
                domain: r.domain,
                status: r.status,
                captured_at: r.captured_at,
            })
            .collect())
    }

    async fn mark_synced_up_to(&self, device_id: &str, up_to_id: i64) -> anyhow::Result<u64> {
        let result = sqlx::query(
            "UPDATE dns_events SET sync_state = 'synced' \
             WHERE device_id = ? AND id <= ? AND sync_state = 'pending'",
        )
        .bind(device_id)
        .bind(up_to_id)
        .execute(&self.pools.write)
        .await?;
        Ok(result.rows_affected())
    }

    async fn delete_up_to(&self, device_id: &str, up_to_id: i64) -> anyhow::Result<u64> {
        let result = sqlx::query("DELETE FROM dns_events WHERE device_id = ? AND id <= ?")
            .bind(device_id)
            .bind(up_to_id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }
}
