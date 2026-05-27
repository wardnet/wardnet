use async_trait::async_trait;

use wardnet_common::stats::StatsTopEntry;

/// A persisted intraday stat sample.
#[derive(Debug, Clone)]
pub struct IntradayStatRow {
    pub metric: String,
    /// Sorted JSON labels object, e.g. `{"client":"x","outcome":"blocked"}`.
    pub labels: String,
    /// Unix seconds, truncated to the minute boundary.
    pub bucket_ts: i64,
    pub value: f64,
    /// `"counter"` or `"gauge"`.
    pub kind: String,
}

/// A persisted hourly stat rollup.
#[derive(Debug, Clone)]
pub struct HourlyStatRow {
    pub metric: String,
    pub labels: String,
    /// Unix seconds, truncated to the hour boundary.
    pub hour_ts: i64,
    pub value: f64,
    /// `"counter"` or `"gauge"`.
    pub kind: String,
}

/// A persisted daily stat rollup.
#[derive(Debug, Clone)]
pub struct DailyStatRow {
    pub metric: String,
    pub labels: String,
    /// Calendar day in `YYYY-MM-DD` (UTC).
    pub day: String,
    pub value: f64,
    pub kind: String,
}

/// Storage for generic pre-aggregated stats.
#[async_trait]
pub trait StatsRepository: Send + Sync {
    // ── Intraday (minute resolution, 25 h retention) ──────────────────────────

    /// Upsert a batch of intraday rows.
    ///
    /// Counter rows accumulate (`value = value + excluded.value`);
    /// gauge rows overwrite (`value = excluded.value`).
    async fn upsert_intraday(&self, rows: &[IntradayStatRow]) -> anyhow::Result<()>;

    /// Delete intraday rows with `bucket_ts < cutoff_ts`.
    async fn trim_intraday(&self, cutoff_ts: i64) -> anyhow::Result<u64>;

    /// Query intraday rows for a metric within a time range.
    ///
    /// If `label_filter` is `Some`, only rows with a matching `labels` value
    /// are returned.
    async fn query_intraday(
        &self,
        metric: &str,
        label_filter: Option<&str>,
        from: i64,
        to: i64,
    ) -> anyhow::Result<Vec<IntradayStatRow>>;

    // ── Hourly (hour resolution, 8 d retention) ───────────────────────────────

    /// Upsert a batch of hourly rows.
    ///
    /// Counter rows accumulate; gauge rows overwrite — same semantics as
    /// `upsert_intraday`.
    async fn upsert_hourly(&self, rows: &[HourlyStatRow]) -> anyhow::Result<()>;

    /// Aggregate `stats_intraday` rows for the hour starting at `hour_ts`
    /// into `stats_hourly`.
    ///
    /// Uses `INSERT OR IGNORE` so repeated calls for the same hour are
    /// idempotent. Returns the number of rows inserted.
    async fn rollup_hourly(&self, hour_ts: i64) -> anyhow::Result<usize>;

    /// Delete hourly rows with `hour_ts < cutoff_ts`.
    async fn trim_hourly(&self, cutoff_ts: i64) -> anyhow::Result<u64>;

    /// Query hourly rows for a metric within a time range.
    async fn query_hourly(
        &self,
        metric: &str,
        label_filter: Option<&str>,
        from: i64,
        to: i64,
    ) -> anyhow::Result<Vec<HourlyStatRow>>;

    // ── Daily (day resolution, 13 mo retention) ───────────────────────────────

    /// Aggregate `stats_intraday` rows for `day` into `stats_daily`.
    ///
    /// Returns the number of rows inserted/updated.
    async fn rollup_daily(&self, day: &str) -> anyhow::Result<usize>;

    /// Delete daily rows with `day < cutoff_day`.
    async fn trim_daily(&self, cutoff_day: &str) -> anyhow::Result<u64>;

    /// Query daily rows for a metric within a date range.
    async fn query_daily(
        &self,
        metric: &str,
        label_filter: Option<&str>,
        from: &str,
        to: &str,
    ) -> anyhow::Result<Vec<DailyStatRow>>;

    // ── Top-N ─────────────────────────────────────────────────────────────────

    /// Sum values grouped by `label_key` via `json_extract`, ordered DESC.
    ///
    /// Queries `stats_intraday` only (bounded by 25 h intraday retention).
    /// Top-N over longer windows is a known limitation tracked separately.
    async fn top_n(
        &self,
        metric: &str,
        label_key: &str,
        from: i64,
        to: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<StatsTopEntry>>;
}
