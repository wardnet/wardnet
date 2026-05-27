//! Real `TunnelLatencyProber` impl: ICMP echo through a specific interface.
//!
//! Linux uses `SO_BINDTODEVICE` (via `surge_ping::ConfigBuilder::interface`)
//! to force the ping socket onto the tunnel interface, so the echo
//! traverses the tunnel rather than the default route. Other platforms
//! return [`LatencyProbeError::Unsupported`] — the production daemon only
//! runs on Linux; macOS/Windows builds reach the mock backend instead.

use std::net::IpAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use wardnetd_services::tunnel::latency_prober::{LatencyProbeError, TunnelLatencyProber};

/// 1.5 s probe budget — keeps each per-tunnel probe well under the 60 s
/// loop cadence even with multiple active tunnels.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// Monotonic counter feeding ICMP id; rotates per probe so overlapping
/// calls cannot collide on the kernel socket.
static NEXT_IDENT: AtomicU16 = AtomicU16::new(0);

/// ICMP echo prober.
///
/// Constructed once with a parsed target [`IpAddr`]; each call to
/// [`Self::probe`] sends a single echo bound to the supplied interface.
pub struct SurgePingTunnelLatencyProber {
    target: IpAddr,
}

impl SurgePingTunnelLatencyProber {
    /// Build a prober for `target`, parsed once at startup so per-probe
    /// calls cannot fail on malformed input.
    #[must_use]
    pub fn new(target: IpAddr) -> Self {
        Self { target }
    }
}

#[async_trait]
impl TunnelLatencyProber for SurgePingTunnelLatencyProber {
    #[cfg(target_os = "linux")]
    async fn probe(&self, interface: &str) -> Result<u64, LatencyProbeError> {
        use surge_ping::{Client, Config, ICMP, PingIdentifier, PingSequence};

        let kind = match self.target {
            IpAddr::V4(_) => ICMP::V4,
            IpAddr::V6(_) => ICMP::V6,
        };
        let config = Config::builder().kind(kind).interface(interface).build();

        let client = Client::new(&config)
            .map_err(|e| LatencyProbeError::Probe(format!("client build failed: {e}")))?;

        let ident = PingIdentifier(NEXT_IDENT.fetch_add(1, Ordering::Relaxed));
        let mut pinger = client.pinger(self.target, ident).await;
        pinger.timeout(PROBE_TIMEOUT);

        match pinger.ping(PingSequence(0), &[]).await {
            Ok((_packet, rtt)) => Ok(u64::try_from(rtt.as_millis()).unwrap_or(u64::MAX)),
            Err(e) => {
                let msg = e.to_string();
                // surge_ping returns its own `Timeout` variant; match on
                // the rendered message to avoid pulling its full error
                // enum into the public surface.
                if msg.to_lowercase().contains("timeout") {
                    Err(LatencyProbeError::Timeout(
                        u64::try_from(PROBE_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
                    ))
                } else {
                    Err(LatencyProbeError::Probe(msg))
                }
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    async fn probe(&self, _interface: &str) -> Result<u64, LatencyProbeError> {
        Err(LatencyProbeError::Unsupported(
            "SO_BINDTODEVICE is Linux-only; tunnel latency probe requires Linux".to_owned(),
        ))
    }
}
