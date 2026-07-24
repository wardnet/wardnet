use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_common::stats::{StatsBucket, StatsTopEntry};

use super::super::stats::{DailyStatRow, HourlyStatRow, IntradayStatRow, StatsRepository};
use crate::db::DbPools;

/// SQLite-backed implementation of [`StatsRepository`].
pub struct SqliteStatsRepository {
    pools: DbPools,
}

impl SqliteStatsRepository {
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

#[derive(sqlx::FromRow)]
struct DbIntradayRow {
    metric: String,
    labels: String,
    bucket_ts: i64,
    value: f64,
    kind: String,
}

impl From<DbIntradayRow> for IntradayStatRow {
    fn from(r: DbIntradayRow) -> Self {
        Self {
            metric: r.metric,
            labels: r.labels,
            bucket_ts: r.bucket_ts,
            value: r.value,
            kind: r.kind,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DbHourlyRow {
    metric: String,
    labels: String,
    hour_ts: i64,
    value: f64,
    kind: String,
}

impl From<DbHourlyRow> for HourlyStatRow {
    fn from(r: DbHourlyRow) -> Self {
        Self {
            metric: r.metric,
            labels: r.labels,
            hour_ts: r.hour_ts,
            value: r.value,
            kind: r.kind,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DbDailyRow {
    metric: String,
    labels: String,
    day: String,
    value: f64,
    kind: String,
}

impl From<DbDailyRow> for DailyStatRow {
    fn from(r: DbDailyRow) -> Self {
        Self {
            metric: r.metric,
            labels: r.labels,
            day: r.day,
            value: r.value,
            kind: r.kind,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DbTopEntry {
    labels: String,
    total: f64,
}

impl From<DbTopEntry> for StatsTopEntry {
    fn from(r: DbTopEntry) -> Self {
        Self {
            labels: r.labels,
            total: r.total,
        }
    }
}

#[async_trait]
impl StatsRepository for SqliteStatsRepository {
    // ── Intraday ──────────────────────────────────────────────────────────────

    async fn upsert_intraday(&self, rows: &[IntradayStatRow]) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut tx = self.pools.write.begin().await?;
        for row in rows {
            // Counter and gauge differ only in the conflict resolution:
            // counters accumulate, gauges overwrite.
            if row.kind == "gauge" {
                sqlx::query(
                    "INSERT INTO stats_intraday (metric, labels, bucket_ts, value, kind) \
                     VALUES (?, ?, ?, ?, ?) \
                     ON CONFLICT (metric, labels, bucket_ts) \
                     DO UPDATE SET value = excluded.value",
                )
            } else {
                sqlx::query(
                    "INSERT INTO stats_intraday (metric, labels, bucket_ts, value, kind) \
                     VALUES (?, ?, ?, ?, ?) \
                     ON CONFLICT (metric, labels, bucket_ts) \
                     DO UPDATE SET value = value + excluded.value",
                )
            }
            .bind(&row.metric)
            .bind(&row.labels)
            .bind(row.bucket_ts)
            .bind(row.value)
            .bind(&row.kind)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn trim_intraday(&self, cutoff_ts: i64) -> anyhow::Result<u64> {
        let res = sqlx::query("DELETE FROM stats_intraday WHERE bucket_ts < ?")
            .bind(cutoff_ts)
            .execute(&self.pools.write)
            .await?;
        Ok(res.rows_affected())
    }

    async fn query_intraday(
        &self,
        metric: &str,
        label_filter: Option<&str>,
        from: i64,
        to: i64,
    ) -> anyhow::Result<Vec<IntradayStatRow>> {
        let rows = if let Some(filter) = label_filter {
            sqlx::query_as::<_, DbIntradayRow>(
                "SELECT metric, labels, bucket_ts, value, kind \
                 FROM stats_intraday \
                 WHERE metric = ? AND labels = ? AND bucket_ts BETWEEN ? AND ? \
                 ORDER BY bucket_ts ASC",
            )
            .bind(metric)
            .bind(filter)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pools.read)
            .await?
        } else {
            sqlx::query_as::<_, DbIntradayRow>(
                "SELECT metric, labels, bucket_ts, value, kind \
                 FROM stats_intraday \
                 WHERE metric = ? AND bucket_ts BETWEEN ? AND ? \
                 ORDER BY bucket_ts ASC",
            )
            .bind(metric)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pools.read)
            .await?
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }

    // ── Hourly ────────────────────────────────────────────────────────────────

    async fn upsert_hourly(&self, rows: &[HourlyStatRow]) -> anyhow::Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut tx = self.pools.write.begin().await?;
        for row in rows {
            if row.kind == "gauge" {
                sqlx::query(
                    "INSERT INTO stats_hourly (metric, labels, hour_ts, value, kind) \
                     VALUES (?, ?, ?, ?, ?) \
                     ON CONFLICT (metric, labels, hour_ts) \
                     DO UPDATE SET value = excluded.value",
                )
            } else {
                sqlx::query(
                    "INSERT INTO stats_hourly (metric, labels, hour_ts, value, kind) \
                     VALUES (?, ?, ?, ?, ?) \
                     ON CONFLICT (metric, labels, hour_ts) \
                     DO UPDATE SET value = value + excluded.value",
                )
            }
            .bind(&row.metric)
            .bind(&row.labels)
            .bind(row.hour_ts)
            .bind(row.value)
            .bind(&row.kind)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn rollup_hourly(&self, hour_ts: i64) -> anyhow::Result<usize> {
        let hour_end = hour_ts + 3600;
        let mut tx = self.pools.write.begin().await?;
        // Counters are summed; gauges take the last observed value in the
        // hour. INSERT OR IGNORE keeps rollup idempotent — re-running for
        // the same hour_ts is a no-op once a row exists.
        let res = sqlx::query(
            "INSERT OR IGNORE INTO stats_hourly (metric, labels, hour_ts, value, kind) \
             SELECT metric, labels, ?, \
                 CASE kind \
                     WHEN 'gauge' THEN (SELECT value FROM stats_intraday si2 \
                                        WHERE si2.metric = si.metric \
                                          AND si2.labels = si.labels \
                                          AND si2.bucket_ts >= ? \
                                          AND si2.bucket_ts < ? \
                                        ORDER BY si2.bucket_ts DESC LIMIT 1) \
                     ELSE SUM(value) \
                 END, \
                 kind \
             FROM stats_intraday si \
             WHERE bucket_ts >= ? AND bucket_ts < ? \
             GROUP BY metric, labels, kind",
        )
        .bind(hour_ts)
        .bind(hour_ts)
        .bind(hour_end)
        .bind(hour_ts)
        .bind(hour_end)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(usize::try_from(res.rows_affected()).unwrap_or(0))
    }

    async fn trim_hourly(&self, cutoff_ts: i64) -> anyhow::Result<u64> {
        let res = sqlx::query("DELETE FROM stats_hourly WHERE hour_ts < ?")
            .bind(cutoff_ts)
            .execute(&self.pools.write)
            .await?;
        Ok(res.rows_affected())
    }

    async fn query_hourly(
        &self,
        metric: &str,
        label_filter: Option<&str>,
        from: i64,
        to: i64,
    ) -> anyhow::Result<Vec<HourlyStatRow>> {
        let rows = if let Some(filter) = label_filter {
            sqlx::query_as::<_, DbHourlyRow>(
                "SELECT metric, labels, hour_ts, value, kind \
                 FROM stats_hourly \
                 WHERE metric = ? AND labels = ? AND hour_ts BETWEEN ? AND ? \
                 ORDER BY hour_ts ASC",
            )
            .bind(metric)
            .bind(filter)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pools.read)
            .await?
        } else {
            sqlx::query_as::<_, DbHourlyRow>(
                "SELECT metric, labels, hour_ts, value, kind \
                 FROM stats_hourly \
                 WHERE metric = ? AND hour_ts BETWEEN ? AND ? \
                 ORDER BY hour_ts ASC",
            )
            .bind(metric)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pools.read)
            .await?
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }

    // ── Daily ─────────────────────────────────────────────────────────────────

    async fn rollup_daily(&self, day: &str) -> anyhow::Result<usize> {
        let mut tx = self.pools.write.begin().await?;
        // Counters are summed; gauges take the last observed value for the day.
        // INSERT OR IGNORE keeps rollup idempotent — re-running for the same
        // day is a no-op once a row exists.
        let res = sqlx::query(
            "INSERT OR IGNORE INTO stats_daily (metric, labels, day, value, kind) \
             SELECT metric, labels, ?, \
                 CASE kind \
                     WHEN 'gauge' THEN (SELECT value FROM stats_intraday si2 \
                                        WHERE si2.metric = si.metric \
                                          AND si2.labels = si.labels \
                                          AND strftime('%Y-%m-%d', si2.bucket_ts, 'unixepoch') = ? \
                                        ORDER BY si2.bucket_ts DESC LIMIT 1) \
                     ELSE SUM(value) \
                 END, \
                 kind \
             FROM stats_intraday si \
             WHERE strftime('%Y-%m-%d', bucket_ts, 'unixepoch') = ? \
             GROUP BY metric, labels, kind",
        )
        .bind(day)
        .bind(day)
        .bind(day)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(usize::try_from(res.rows_affected()).unwrap_or(0))
    }

    async fn trim_daily(&self, cutoff_day: &str) -> anyhow::Result<u64> {
        let res = sqlx::query("DELETE FROM stats_daily WHERE day < ?")
            .bind(cutoff_day)
            .execute(&self.pools.write)
            .await?;
        Ok(res.rows_affected())
    }

    async fn query_daily(
        &self,
        metric: &str,
        label_filter: Option<&str>,
        from: &str,
        to: &str,
    ) -> anyhow::Result<Vec<DailyStatRow>> {
        let rows = if let Some(filter) = label_filter {
            sqlx::query_as::<_, DbDailyRow>(
                "SELECT metric, labels, day, value, kind \
                 FROM stats_daily \
                 WHERE metric = ? AND labels = ? AND day BETWEEN ? AND ? \
                 ORDER BY day ASC",
            )
            .bind(metric)
            .bind(filter)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pools.read)
            .await?
        } else {
            sqlx::query_as::<_, DbDailyRow>(
                "SELECT metric, labels, day, value, kind \
                 FROM stats_daily \
                 WHERE metric = ? AND day BETWEEN ? AND ? \
                 ORDER BY day ASC",
            )
            .bind(metric)
            .bind(from)
            .bind(to)
            .fetch_all(&self.pools.read)
            .await?
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }

    // ── Top-N ─────────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    async fn top_n(
        &self,
        metric: &str,
        label_key: &str,
        fallback_label_key: Option<&str>,
        bucket: StatsBucket,
        from: i64,
        to: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<StatsTopEntry>> {
        // json_extract is covered by the expression indexes for the known
        // label keys (outcome, domain, client, device_id, company) on each
        // tier. Unknown keys — and the COALESCE form — fall back to a scan of
        // the (pre-aggregated) tier table but still return correct results.
        //
        // The tier is chosen by `bucket` so top-N spans the same window the
        // caller charts: intraday for short ranges, hourly/daily for longer
        // ones. `from`/`to` are Unix seconds throughout; the daily tier keys on
        // calendar day, so its predicate converts the bounds with
        // `date(?, 'unixepoch')`.
        //
        // With a fallback key one group can span rows with different label
        // strings (a device seen under several IPs); MAX(labels) keeps the
        // representative row deterministic instead of leaving SQLite to
        // pick an arbitrary member.
        let (table, time_pred) = match bucket {
            StatsBucket::Minute => ("stats_intraday", "bucket_ts BETWEEN ? AND ?"),
            StatsBucket::Hour => ("stats_hourly", "hour_ts BETWEEN ? AND ?"),
            StatsBucket::Day => (
                "stats_daily",
                "day BETWEEN date(?, 'unixepoch') AND date(?, 'unixepoch')",
            ),
        };
        let key_path = format!("$.{label_key}");
        let fallback_path = fallback_label_key.map(|f| format!("$.{f}"));
        let group_expr = match &fallback_path {
            Some(_) => "COALESCE(json_extract(labels, ?), json_extract(labels, ?))",
            None => "json_extract(labels, ?)",
        };
        let sql = format!(
            "SELECT MAX(labels) AS labels, SUM(value) AS total \
             FROM {table} \
             WHERE metric = ? AND {time_pred} \
               AND {group_expr} IS NOT NULL \
             GROUP BY {group_expr} \
             ORDER BY total DESC \
             LIMIT ?",
        );
        let mut query = sqlx::query_as::<_, DbTopEntry>(sqlx::AssertSqlSafe(sql))
            .bind(metric)
            .bind(from)
            .bind(to);
        // The grouping expression appears twice (IS NOT NULL + GROUP BY);
        // bind its path parameter(s) once per occurrence.
        for _ in 0..2 {
            query = query.bind(&key_path);
            if let Some(ref fallback) = fallback_path {
                query = query.bind(fallback);
            }
        }
        let rows = query.bind(limit).fetch_all(&self.pools.read).await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
