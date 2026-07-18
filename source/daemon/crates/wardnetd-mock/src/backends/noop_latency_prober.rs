//! Deterministic [`TunnelLatencyProber`] implementation for the mock server.
//!
//! Returns a stable per-interface RTT in the 25–80 ms range so the
//! frontend's tunnel latency chart has plausible data without making
//! real ICMP echoes (which `make run-dev` cannot do on macOS anyway).

use std::time::Duration;

use async_trait::async_trait;

use wardnetd_services::tunnel::latency_prober::{LatencyProbeError, TunnelLatencyProber};

/// Synthetic-latency prober for the mock daemon.
pub struct NoopLatencyProber;

impl NoopLatencyProber {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoopLatencyProber {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TunnelLatencyProber for NoopLatencyProber {
    async fn probe(&self, interface_name: Option<&str>) -> Result<u64, LatencyProbeError> {
        // 10 ms feigned send time so back-to-back probes don't all
        // resolve in the same tokio poll.
        tokio::time::sleep(Duration::from_millis(10)).await;
        // `None` is the direct/WAN leg of a speed test; give it a stable
        // low-ish baseline so the tunnel always looks slightly slower.
        Ok(synthetic_rtt(interface_name.unwrap_or("<direct>")))
    }
}

/// Hash the interface name into a stable RTT in the 25–80 ms range so
/// each tunnel gets a distinct but plausible latency value.
pub(crate) fn synthetic_rtt(interface_name: &str) -> u64 {
    let h: u64 = interface_name.bytes().fold(0_u64, |acc, b| {
        acc.wrapping_mul(131).wrapping_add(u64::from(b))
    });
    25 + (h % 56)
}

