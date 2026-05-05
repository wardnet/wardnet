use async_trait::async_trait;

/// One sample of intraday throughput for a single tunnel.
///
/// Each row stores the *delta* of bytes observed between the previous
/// sample and `ts`. The client renders bytes/sec by dividing by the
/// configured sample interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntradayMetricRow {
    pub tunnel_id: String,
    /// Unix timestamp (seconds) of the *end* of the interval the delta covers.
    pub ts: i64,
    pub bytes_tx_delta: i64,
    pub bytes_rx_delta: i64,
}

/// Daily rollup of throughput totals for a single tunnel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyMetricRow {
    pub tunnel_id: String,
    /// Calendar day in `YYYY-MM-DD` (UTC).
    pub day: String,
    pub bytes_tx_total: i64,
    pub bytes_rx_total: i64,
}

/// Storage for per-tunnel throughput history.
///
/// The intraday table is written by the tunnel-stats poll loop after
/// per-sample decimation, and trimmed by the rollup runner. The daily
/// table is populated by the runner from intraday rows.
#[async_trait]
pub trait TunnelMetricsRepository: Send + Sync {
    /// Insert a single intraday sample.
    async fn insert_intraday(&self, row: &IntradayMetricRow) -> anyhow::Result<()>;

    /// Bulk-insert intraday samples (used by mock seeding and backup restore).
    async fn insert_intraday_batch(&self, rows: &[IntradayMetricRow]) -> anyhow::Result<()>;

    /// Bulk-insert daily samples (used by mock seeding and backup restore).
    async fn insert_daily_batch(&self, rows: &[DailyMetricRow]) -> anyhow::Result<()>;

    /// Return all intraday rows for a tunnel where `from_ts <= ts <= to_ts`,
    /// ordered ascending by `ts`.
    async fn query_intraday(
        &self,
        tunnel_id: &str,
        from_ts: i64,
        to_ts: i64,
    ) -> anyhow::Result<Vec<IntradayMetricRow>>;

    /// Return all daily rows for a tunnel where `from_day <= day <= to_day`,
    /// ordered ascending by `day`.
    async fn query_daily(
        &self,
        tunnel_id: &str,
        from_day: &str,
        to_day: &str,
    ) -> anyhow::Result<Vec<DailyMetricRow>>;

    /// Days that have at least one intraday sample but no daily row.
    ///
    /// Returns `YYYY-MM-DD` strings (UTC) ordered ascending. Used by the
    /// rollup runner to discover work.
    async fn days_pending_rollup(&self, before_day: &str) -> anyhow::Result<Vec<String>>;

    /// Aggregate intraday rows for a single calendar day into the daily
    /// table. No-op (returns 0) if the daily row already exists.
    ///
    /// Returns the number of daily rows inserted (0 or N tunnels).
    async fn rollup_day(&self, day: &str) -> anyhow::Result<usize>;

    /// Delete intraday rows older than `cutoff_ts`.
    async fn trim_intraday(&self, cutoff_ts: i64) -> anyhow::Result<u64>;

    /// Delete daily rows older than `cutoff_day`.
    async fn trim_daily(&self, cutoff_day: &str) -> anyhow::Result<u64>;

    /// Return all intraday rows (used by the backup archiver).
    async fn export_intraday(&self) -> anyhow::Result<Vec<IntradayMetricRow>>;

    /// Return all daily rows (used by the backup archiver).
    async fn export_daily(&self) -> anyhow::Result<Vec<DailyMetricRow>>;
}
