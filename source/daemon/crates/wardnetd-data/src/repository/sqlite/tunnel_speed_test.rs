use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_common::speed_test::TunnelSpeedTestResult;

use super::super::TunnelSpeedTestRepository;
use super::super::tunnel_speed_test::SpeedTestRow;
use crate::db::DbPools;

/// SQLite-backed implementation of [`TunnelSpeedTestRepository`].
pub struct SqliteTunnelSpeedTestRepository {
    pools: DbPools,
}

impl SqliteTunnelSpeedTestRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    /// Create a new repository with split reader/writer pools.
    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

/// Raw row from the `tunnel_speed_test_results` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct DbSpeedTestRow {
    id: String,
    tunnel_id: String,
    direct_throughput_mbps: f64,
    tunnel_throughput_mbps: f64,
    direct_latency_ms: f64,
    tunnel_latency_ms: f64,
    direct_jitter_ms: f64,
    tunnel_jitter_ms: f64,
    tested_at: String,
}

impl DbSpeedTestRow {
    /// Convert the raw database row into a domain [`TunnelSpeedTestResult`].
    fn into_result(self) -> anyhow::Result<TunnelSpeedTestResult> {
        Ok(TunnelSpeedTestResult {
            id: self.id.parse()?,
            tunnel_id: self.tunnel_id.parse()?,
            direct_throughput_mbps: self.direct_throughput_mbps,
            tunnel_throughput_mbps: self.tunnel_throughput_mbps,
            direct_latency_ms: self.direct_latency_ms,
            tunnel_latency_ms: self.tunnel_latency_ms,
            direct_jitter_ms: self.direct_jitter_ms,
            tunnel_jitter_ms: self.tunnel_jitter_ms,
            tested_at: self.tested_at.parse()?,
        })
    }
}

#[async_trait]
impl TunnelSpeedTestRepository for SqliteTunnelSpeedTestRepository {
    async fn insert(&self, row: &SpeedTestRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO tunnel_speed_test_results (id, tunnel_id, \
             direct_throughput_mbps, tunnel_throughput_mbps, \
             direct_latency_ms, tunnel_latency_ms, \
             direct_jitter_ms, tunnel_jitter_ms, tested_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.tunnel_id)
        .bind(row.direct_throughput_mbps)
        .bind(row.tunnel_throughput_mbps)
        .bind(row.direct_latency_ms)
        .bind(row.tunnel_latency_ms)
        .bind(row.direct_jitter_ms)
        .bind(row.tunnel_jitter_ms)
        .bind(&row.tested_at)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn find_recent(
        &self,
        tunnel_id: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<TunnelSpeedTestResult>> {
        let rows = sqlx::query_as::<_, DbSpeedTestRow>(
            "SELECT id, tunnel_id, direct_throughput_mbps, tunnel_throughput_mbps, \
             direct_latency_ms, tunnel_latency_ms, direct_jitter_ms, tunnel_jitter_ms, \
             tested_at FROM tunnel_speed_test_results \
             WHERE tunnel_id = ? ORDER BY tested_at DESC LIMIT ?",
        )
        .bind(tunnel_id)
        .bind(limit)
        .fetch_all(&self.pools.read)
        .await?;
        rows.into_iter().map(DbSpeedTestRow::into_result).collect()
    }
}
