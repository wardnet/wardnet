use async_trait::async_trait;
use wardnet_common::egress_path::PathProbeOutcome;

/// Where the probe connects.
///
/// An IP literal and a port, never a hostname: resolving a name would make a
/// DNS failure indistinguishable from a connect failure, which is precisely the
/// distinction this subsystem exists to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeTarget {
    pub host: String,
    pub port: u16,
}

impl Default for ProbeTarget {
    /// Cloudflare's `1.1.1.1:443`.
    ///
    /// Already the default endpoint for the tunnel exit probe
    /// (`tunnel.test_probe_url`), so this adds no new third party. Anycast, so
    /// it is close from anywhere a tunnel exits, and it terminates TLS — which
    /// the transfer stage needs in order to put full-MTU segments on the wire.
    fn default() -> Self {
        Self {
            host: "1.1.1.1".to_owned(),
            port: 443,
        }
    }
}

/// Completes a two-stage probe over one egress path.
///
/// When `interface_name` is `Some`, implementations must bind the outbound
/// socket to it (Linux: `SO_BINDTODEVICE`) so the probe traverses that tunnel
/// rather than the default route. `None` probes **unbound**, over the default
/// route — the direct/WAN path. This mirrors
/// [`TunnelLatencyProber`](crate::tunnel::TunnelLatencyProber) deliberately:
/// the same `Option` already encodes exactly the set of egress paths.
///
/// Returns an outcome rather than a `Result`: a failed probe is data, not an
/// error. The whole point is to record *how* a path failed.
#[async_trait]
pub trait EgressPathProber: Send + Sync {
    async fn probe(&self, interface_name: Option<&str>) -> PathProbeOutcome;
}
