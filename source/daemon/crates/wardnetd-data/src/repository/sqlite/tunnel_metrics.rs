use async_trait::async_trait;
use sqlx::SqlitePool;

use super::super::tunnel_metrics::{DailyMetricRow, IntradayMetricRow, TunnelMetricsRepository};

/// SQLite-backed implementation of [`TunnelMetricsRepository`].
pub struct SqliteTunnelMetricsRepository {
    pool: SqlitePool,
}

impl SqliteTunnelMetricsRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
struct DbIntradayRow {
    tunnel_id: String,
    ts: i64,
    bytes_tx_delta: i64,
    bytes_rx_delta: i64,
}

impl From<DbIntradayRow> for IntradayMetricRow {
    fn from(r: DbIntradayRow) -> Self {
        Self {
            tunnel_id: r.tunnel_id,
            ts: r.ts,
            bytes_tx_delta: r.bytes_tx_delta,
            bytes_rx_delta: r.bytes_rx_delta,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DbDailyRow {
    tunnel_id: String,
    day: String,
    bytes_tx_total: i64,
    bytes_rx_total: i64,
}

impl From<DbDailyRow> for DailyMetricRow {
    fn from(r: DbDailyRow) -> Self {
        Self {
            tunnel_id: r.tunnel_id,
            day: r.day,
            bytes_tx_total: r.bytes_tx_total,
            bytes_rx_total: r.bytes_rx_total,
        }
    }
}

#[async_trait]
impl TunnelMetricsRepository for SqliteTunnelMetricsRepository {
    async fn insert_intraday(&self, row: &IntradayMetricRow) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO tunnel_metrics_intraday \
             (tunnel_id, ts, bytes_tx_delta, bytes_rx_delta) VALUES (?, ?, ?, ?)",
        )
        .bind(&row.tunnel_id)
        .bind(row.ts)
        .bind(row.bytes_tx_delta)
        .bind(row.bytes_rx_delta)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn insert_intraday_batch(&self, rows: &[IntradayMetricRow]) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for row in rows {
            sqlx::query(
                "INSERT OR REPLACE INTO tunnel_metrics_intraday \
                 (tunnel_id, ts, bytes_tx_delta, bytes_rx_delta) VALUES (?, ?, ?, ?)",
            )
            .bind(&row.tunnel_id)
            .bind(row.ts)
            .bind(row.bytes_tx_delta)
            .bind(row.bytes_rx_delta)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn insert_daily_batch(&self, rows: &[DailyMetricRow]) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut tx = self.pool.begin().await?;
        for row in rows {
            sqlx::query(
                "INSERT OR REPLACE INTO tunnel_metrics_daily \
                 (tunnel_id, day, bytes_tx_total, bytes_rx_total) VALUES (?, ?, ?, ?)",
            )
            .bind(&row.tunnel_id)
            .bind(&row.day)
            .bind(row.bytes_tx_total)
            .bind(row.bytes_rx_total)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn query_intraday(
        &self,
        tunnel_id: &str,
        from_ts: i64,
        to_ts: i64,
    ) -> anyhow::Result<Vec<IntradayMetricRow>> {
        let rows = sqlx::query_as::<_, DbIntradayRow>(
            "SELECT tunnel_id, ts, bytes_tx_delta, bytes_rx_delta \
             FROM tunnel_metrics_intraday \
             WHERE tunnel_id = ? AND ts BETWEEN ? AND ? \
             ORDER BY ts ASC",
        )
        .bind(tunnel_id)
        .bind(from_ts)
        .bind(to_ts)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn query_daily(
        &self,
        tunnel_id: &str,
        from_day: &str,
        to_day: &str,
    ) -> anyhow::Result<Vec<DailyMetricRow>> {
        let rows = sqlx::query_as::<_, DbDailyRow>(
            "SELECT tunnel_id, day, bytes_tx_total, bytes_rx_total \
             FROM tunnel_metrics_daily \
             WHERE tunnel_id = ? AND day BETWEEN ? AND ? \
             ORDER BY day ASC",
        )
        .bind(tunnel_id)
        .bind(from_day)
        .bind(to_day)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn days_pending_rollup(&self, before_day: &str) -> anyhow::Result<Vec<String>> {
        // Calendar days that have at least one intraday row strictly before
        // `before_day`, but no daily row yet. `before_day` is the current
        // (UTC) day — we never roll up the in-progress day.
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT strftime('%Y-%m-%d', ts, 'unixepoch') AS d \
             FROM tunnel_metrics_intraday \
             WHERE strftime('%Y-%m-%d', ts, 'unixepoch') < ? \
             AND NOT EXISTS ( \
                 SELECT 1 FROM tunnel_metrics_daily \
                 WHERE tunnel_metrics_daily.day = strftime('%Y-%m-%d', tunnel_metrics_intraday.ts, 'unixepoch') \
             ) \
             ORDER BY d ASC",
        )
        .bind(before_day)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    async fn rollup_day(&self, day: &str) -> anyhow::Result<usize> {
        let mut tx = self.pool.begin().await?;
        // INSERT OR IGNORE keeps the operation idempotent: re-running
        // rollup_day for the same day is a no-op once rows exist.
        let res = sqlx::query(
            "INSERT OR IGNORE INTO tunnel_metrics_daily (tunnel_id, day, bytes_tx_total, bytes_rx_total) \
             SELECT tunnel_id, ?, SUM(bytes_tx_delta), SUM(bytes_rx_delta) \
             FROM tunnel_metrics_intraday \
             WHERE strftime('%Y-%m-%d', ts, 'unixepoch') = ? \
             GROUP BY tunnel_id",
        )
        .bind(day)
        .bind(day)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(usize::try_from(res.rows_affected()).unwrap_or(0))
    }

    async fn trim_intraday(&self, cutoff_ts: i64) -> anyhow::Result<u64> {
        let res = sqlx::query("DELETE FROM tunnel_metrics_intraday WHERE ts < ?")
            .bind(cutoff_ts)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    async fn trim_daily(&self, cutoff_day: &str) -> anyhow::Result<u64> {
        let res = sqlx::query("DELETE FROM tunnel_metrics_daily WHERE day < ?")
            .bind(cutoff_day)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    async fn export_intraday(&self) -> anyhow::Result<Vec<IntradayMetricRow>> {
        let rows = sqlx::query_as::<_, DbIntradayRow>(
            "SELECT tunnel_id, ts, bytes_tx_delta, bytes_rx_delta \
             FROM tunnel_metrics_intraday ORDER BY tunnel_id, ts",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn export_daily(&self) -> anyhow::Result<Vec<DailyMetricRow>> {
        let rows = sqlx::query_as::<_, DbDailyRow>(
            "SELECT tunnel_id, day, bytes_tx_total, bytes_rx_total \
             FROM tunnel_metrics_daily ORDER BY tunnel_id, day",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
