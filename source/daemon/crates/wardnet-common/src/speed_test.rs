//! Per-tunnel speed test result types.
//!
//! A speed test measures throughput, latency and jitter twice per run — once
//! over the **direct** (unbound/WAN) path and once **through the tunnel** — so
//! the admin can see how much of their line the VPN preserves (retention)
//! rather than an isolated tunnel number. Both legs are persisted in a single
//! [`TunnelSpeedTestResult`] row so the comparison stays apples-to-apples
//! (measured seconds apart under the same line conditions).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One completed speed test: the direct (WAN) baseline and the tunnel result,
/// measured back-to-back in a single run.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelSpeedTestResult {
    pub id: Uuid,
    pub tunnel_id: Uuid,
    /// Download throughput over the direct (unbound/WAN) path, in megabits/s.
    pub direct_throughput_mbps: f64,
    /// Download throughput through the tunnel interface, in megabits/s.
    pub tunnel_throughput_mbps: f64,
    /// Median ICMP round-trip latency over the direct path, in milliseconds.
    pub direct_latency_ms: f64,
    /// Median ICMP round-trip latency through the tunnel, in milliseconds.
    pub tunnel_latency_ms: f64,
    /// Latency jitter (sample standard deviation) over the direct path, in ms.
    pub direct_jitter_ms: f64,
    /// Latency jitter (sample standard deviation) through the tunnel, in ms.
    pub tunnel_jitter_ms: f64,
    pub tested_at: DateTime<Utc>,
}

/// History of recent speed tests for one tunnel, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TunnelSpeedTestHistoryResponse {
    pub results: Vec<TunnelSpeedTestResult>,
}
