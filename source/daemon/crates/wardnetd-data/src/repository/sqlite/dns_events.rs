use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::dns_events::{DnsCaptureStats, DnsEventsRepository};

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
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO dns_events (device_id, domain, status, captured_at) VALUES (?, ?, ?, ?)",
        )
        .bind(device_id)
        .bind(domain)
        .bind(status)
        .bind(captured_at)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn stats_for_device(&self, device_id: &str) -> anyhow::Result<DnsCaptureStats> {
        let row_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM dns_events WHERE device_id = ?")
                .bind(device_id)
                .fetch_one(&self.pools.read)
                .await?;

        let size_bytes = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(LENGTH(domain)), 0) FROM dns_events WHERE device_id = ?",
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
}
