use async_trait::async_trait;

/// Failure modes for [`ThroughputTester::download`].
#[derive(Debug, thiserror::Error)]
pub enum ThroughputError {
    /// The download could not be started or completed (socket error, no
    /// route, connection reset, non-success HTTP status).
    #[error("throughput download failed: {0}")]
    Download(String),

    /// The download did not finish inside the time budget.
    #[error("throughput download timed out after {0} ms")]
    Timeout(u64),

    /// The tester is not implemented for this build/platform. Binding a
    /// socket to a tunnel interface (`SO_BINDTODEVICE`) is Linux-only;
    /// macOS/Windows builds only ever see this through the mock backend.
    #[error("throughput testing unsupported on this platform: {0}")]
    Unsupported(String),
}

/// Result of a single throughput download.
#[derive(Debug, Clone, Copy)]
pub struct ThroughputMeasurement {
    /// Measured download throughput in megabits per second.
    pub mbps: f64,
}

/// Measures sustained download throughput against a configured URL.
///
/// The download URL is fixed at construction (mirroring
/// [`TunnelExitProbe`](crate::tunnel::exit_probe::TunnelExitProbe) and
/// [`TunnelLatencyProber`](crate::tunnel::latency_prober::TunnelLatencyProber)).
/// When `interface_name` is `Some`, implementations bind the outbound socket
/// to it (Linux: `SO_BINDTODEVICE`) so the download traverses that tunnel;
/// when `None`, the download runs **unbound** over the default route — the
/// direct/WAN baseline the speed test compares the tunnel against.
///
/// Implementations should measure *sustained* throughput rather than timing
/// a single fixed-size download end to end: a single-shot single-connection
/// transfer is skewed by connection-setup time and TCP slow-start (worse at
/// higher RTT, e.g. through a tunnel) and by the single-flow
/// bandwidth-delay-product ceiling. The daemon's real implementation
/// (`HttpThroughputTester` in the `wardnetd` crate) addresses this by
/// running several concurrent streams over a fixed measurement window,
/// discarding an initial warm-up period.
#[async_trait]
pub trait ThroughputTester: Send + Sync {
    /// Download the configured payload through `interface_name` (or the
    /// direct/WAN path when `None`) and return the measured throughput.
    async fn download(
        &self,
        interface_name: Option<&str>,
    ) -> Result<ThroughputMeasurement, ThroughputError>;
}
