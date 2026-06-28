//! Deterministic [`ThroughputTester`] implementation for the mock server.
//!
//! Returns stable, plausible throughput numbers so the frontend's speed
//! test UI has data to render without performing a real download (which
//! `make run-dev` cannot do meaningfully on macOS). The direct (WAN) leg
//! reports a higher number than the tunnel leg so the comparison always
//! shows the tunnel keeping most — but not all — of the line.

use std::time::Duration;

use async_trait::async_trait;

use wardnetd_services::tunnel::throughput_tester::{
    ThroughputError, ThroughputMeasurement, ThroughputTester,
};

/// Synthetic direct (WAN) throughput, megabits/s.
const DIRECT_MBPS: f64 = 94.0;
/// Synthetic through-tunnel throughput, megabits/s (≈90% of direct).
const TUNNEL_MBPS: f64 = 85.0;

/// Synthetic-throughput tester for the mock daemon.
pub struct NoopThroughputTester;

impl NoopThroughputTester {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoopThroughputTester {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ThroughputTester for NoopThroughputTester {
    async fn download(
        &self,
        interface: Option<&str>,
    ) -> Result<ThroughputMeasurement, ThroughputError> {
        // Brief feigned download so the job spends visible time per leg and
        // the UI's progress bar has something to show.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let mbps = if interface.is_some() {
            TUNNEL_MBPS
        } else {
            DIRECT_MBPS
        };
        Ok(ThroughputMeasurement { mbps })
    }
}
