use async_trait::async_trait;
use wardnet_common::speed_test::TunnelSpeedTestResult;

/// Insert-only data transfer object for a completed speed test.
///
/// All fields map directly to columns on `tunnel_speed_test_results`. The
/// service layer constructs this after measuring both the direct (WAN) and
/// tunnel legs of a run.
#[derive(Debug, Clone)]
pub struct SpeedTestRow {
    /// UUID primary key.
    pub id: String,
    /// Owning tunnel UUID.
    pub tunnel_id: String,
    /// Direct (unbound/WAN) download throughput, megabits/s.
    pub direct_throughput_mbps: f64,
    /// Through-tunnel download throughput, megabits/s.
    pub tunnel_throughput_mbps: f64,
    /// Direct median round-trip latency, milliseconds.
    pub direct_latency_ms: f64,
    /// Through-tunnel median round-trip latency, milliseconds.
    pub tunnel_latency_ms: f64,
    /// Direct latency jitter (sample stddev), milliseconds.
    pub direct_jitter_ms: f64,
    /// Through-tunnel latency jitter (sample stddev), milliseconds.
    pub tunnel_jitter_ms: f64,
    /// ISO 8601 timestamp of when the test completed.
    pub tested_at: String,
}

/// Data access for per-tunnel speed test history.
///
/// Append-only: rows are inserted by the speed test job and never updated.
/// History is not trimmed — [`find_recent`](Self::find_recent) bounds reads
/// instead, so the table stays small in practice without discarding data.
#[async_trait]
pub trait TunnelSpeedTestRepository: Send + Sync {
    /// Insert a completed speed test result.
    async fn insert(&self, row: &SpeedTestRow) -> anyhow::Result<()>;

    /// Return the most recent `limit` results for a tunnel, newest first.
    async fn find_recent(
        &self,
        tunnel_id: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<TunnelSpeedTestResult>>;
}
