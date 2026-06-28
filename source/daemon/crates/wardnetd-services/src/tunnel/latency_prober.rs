use async_trait::async_trait;

/// Failure modes for [`TunnelLatencyProber::probe`].
#[derive(Debug, thiserror::Error)]
pub enum LatencyProbeError {
    /// The ICMP echo could not be sent through the interface (socket
    /// error, no route, peer unreachable).
    #[error("latency probe failed: {0}")]
    Probe(String),

    /// The echo was sent but no reply arrived inside the time budget.
    #[error("latency probe timed out after {0} ms")]
    Timeout(u64),

    /// The probe is not implemented for this build/platform. Raw ICMP
    /// plus `SO_BINDTODEVICE` is Linux-only; macOS/Windows builds only
    /// ever see this through the mock backend.
    #[error("latency probe unsupported on this platform: {0}")]
    Unsupported(String),
}

/// Sends a single ICMP echo and returns the observed round-trip time.
///
/// When `interface_name` is `Some`, implementations must bind the outbound
/// socket to it (Linux: `SO_BINDTODEVICE`) so the probe traverses that tunnel
/// rather than the default route. When `None`, the probe is sent **unbound**
/// over the default route (the direct/WAN path) — used by the speed test to
/// establish a baseline to compare the tunnel against.
#[async_trait]
pub trait TunnelLatencyProber: Send + Sync {
    /// Probe `interface_name` (or the direct/WAN path when `None`) and return
    /// the round-trip time in milliseconds.
    async fn probe(&self, interface_name: Option<&str>) -> Result<u64, LatencyProbeError>;
}
