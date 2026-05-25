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
    /// Upsert a batch of intraday rows.
    ///
    /// Counter rows accumulate (`value = value + excluded.value`);
    /// gauge rows overwrite (`value = excluded.value`).
    async fn upsert_intraday(&self, rows: &[IntradayStatRow]) -> anyhow::Result<()>;

    /// Aggregate `stats_intraday` rows for `day` into `stats_daily`.
    ///
    /// Returns the number of rows inserted/updated.
    async fn rollup_daily(&self, day: &str) -> anyhow::Result<usize>;

    /// Delete intraday rows with `bucket_ts < cutoff_ts`.
    async fn trim_intraday(&self, cutoff_ts: i64) -> anyhow::Result<u64>;

    /// Delete daily rows with `day < cutoff_day`.
    async fn trim_daily(&self, cutoff_day: &str) -> anyhow::Result<u64>;

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

    /// Query daily rows for a metric within a date range.
    async fn query_daily(
        &self,
        metric: &str,
        label_filter: Option<&str>,
        from: &str,
        to: &str,
    ) -> anyhow::Result<Vec<DailyStatRow>>;

    /// Sum values grouped by `label_key` via `json_extract`, ordered DESC.
    async fn top_n(
        &self,
        metric: &str,
        label_key: &str,
        from: i64,
        to: i64,
        limit: u32,
    ) -> anyhow::Result<Vec<StatsTopEntry>>;
}
