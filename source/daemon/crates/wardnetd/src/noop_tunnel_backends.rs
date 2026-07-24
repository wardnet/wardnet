//! Deterministic no-op tunnel backends for end-to-end testing.
//!
//! The real daemon runs unchanged in production; these are only wired when
//! `[test] stub_tunnel_backends = true` is set in `wardnet.toml` (see
//! [`wardnet_common::config::TestConfig`]). The end-to-end web-ui suite runs
//! the real binary against a compose stack that has no live `WireGuard` tunnel
//! and no guaranteed internet egress, so the speed-test / tunnel-test
//! measurement path has nothing real to measure. Swapping in these no-op
//! backends makes that path deterministic: fixed throughput and latency
//! numbers, an instant (fabricated) handshake, and no kernel or network I/O.
//! They mirror the `wardnetd-mock` backends of the same names, rounded so a
//! Playwright spec can assert the rendered strings exactly: the tunnel keeps
//! 90% of a 100 Mbps line and doubles a 10 ms baseline latency.
//!
//! Like the real network-bound backends it stands in for, this module is
//! exercised end-to-end rather than by unit tests, so it is excluded from
//! coverage. The `noop_` filename prefix is deliberate: it matches the
//! `noop_.*\.rs` pattern already in the top-level `Makefile`'s `COV_IGNORE`
//! (the CI coverage gate), so no per-file entry is added to that shared regex
//! — the same convention the `wardnetd-mock` `noop_*` backends rely on.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use wardnetd_data::repository::TunnelRepository;
use wardnetd_services::tunnel::exit_probe::{ExitInfo, ProbeError, TunnelExitProbe};
use wardnetd_services::tunnel::latency_prober::{LatencyProbeError, TunnelLatencyProber};
use wardnetd_services::tunnel::throughput_tester::{
    ThroughputError, ThroughputMeasurement, ThroughputTester,
};
use wardnetd_services::tunnel::{CreateTunnelParams, TunnelInterface, TunnelStats};

/// Synthetic direct (WAN) throughput, megabits/s.
const DIRECT_MBPS: f64 = 100.0;
/// Synthetic through-tunnel throughput, megabits/s (90% of direct).
const TUNNEL_MBPS: f64 = 90.0;
/// Synthetic direct (WAN) round-trip latency, milliseconds.
const DIRECT_LATENCY_MS: u64 = 10;
/// Synthetic through-tunnel round-trip latency, milliseconds.
const TUNNEL_LATENCY_MS: u64 = 20;

/// A [`TunnelInterface`] that performs no kernel operations and reports a
/// fresh handshake, so a speed test can bring a `Down` tunnel "up" (a no-op)
/// and clear the handshake-readiness gate without a real peer.
#[derive(Debug, Default, Clone)]
pub struct NoopTunnelInterface;

#[async_trait]
impl TunnelInterface for NoopTunnelInterface {
    async fn create(&self, _params: CreateTunnelParams) -> anyhow::Result<()> {
        Ok(())
    }

    async fn bring_up(&self, _interface_name: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn tear_down(&self, _interface_name: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn remove(&self, _interface_name: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn get_stats(&self, _interface_name: &str) -> anyhow::Result<Option<TunnelStats>> {
        // A current handshake clears `await_fresh_handshake` immediately; the
        // byte counters are non-zero so per-poll deltas look plausible.
        Ok(Some(TunnelStats {
            bytes_tx: 1_000_000,
            bytes_rx: 3_000_000,
            last_handshake: Some(chrono::Utc::now()),
        }))
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }
}

/// A [`ThroughputTester`] that returns a fixed rate per leg without any
/// download: the tunnel leg (`Some(iface)`) is lower than the direct/WAN leg
/// (`None`), so the comparison always shows retained-but-not-full bandwidth.
pub struct NoopThroughputTester;

#[async_trait]
impl ThroughputTester for NoopThroughputTester {
    async fn download(
        &self,
        interface: Option<&str>,
    ) -> Result<ThroughputMeasurement, ThroughputError> {
        // A brief feigned transfer so the job spends visible time per leg and
        // a concurrent run reliably collides with the in-flight guard.
        tokio::time::sleep(Duration::from_millis(200)).await;
        let mbps = if interface.is_some() {
            TUNNEL_MBPS
        } else {
            DIRECT_MBPS
        };
        Ok(ThroughputMeasurement { mbps })
    }
}

/// A [`TunnelLatencyProber`] that returns a fixed RTT per leg. Every sample in
/// a leg is identical, so the summarised median is the constant and the jitter
/// is zero — deterministic across runs.
pub struct NoopLatencyProber;

#[async_trait]
impl TunnelLatencyProber for NoopLatencyProber {
    async fn probe(&self, interface_name: Option<&str>) -> Result<u64, LatencyProbeError> {
        tokio::time::sleep(Duration::from_millis(5)).await;
        Ok(if interface_name.is_some() {
            TUNNEL_LATENCY_MS
        } else {
            DIRECT_LATENCY_MS
        })
    }
}

/// A [`TunnelExitProbe`] that returns a synthetic public IP plus the tunnel's
/// stored country code, looked up by interface name. No egress; mirrors the
/// mock daemon so the tunnel-test flow is deterministic too.
pub struct NoopExitProbe {
    tunnels: Arc<dyn TunnelRepository>,
}

impl NoopExitProbe {
    #[must_use]
    pub fn new(tunnels: Arc<dyn TunnelRepository>) -> Self {
        Self { tunnels }
    }
}

#[async_trait]
impl TunnelExitProbe for NoopExitProbe {
    async fn probe(&self, interface: &str) -> Result<ExitInfo, ProbeError> {
        tokio::time::sleep(Duration::from_millis(50)).await;

        let tunnels = self
            .tunnels
            .find_all()
            .await
            .map_err(|e| ProbeError::Connect(format!("stub probe repo lookup failed: {e}")))?;
        let tunnel = tunnels
            .into_iter()
            .find(|t| t.interface_name == interface)
            .ok_or_else(|| {
                ProbeError::Connect(format!(
                    "stub probe: no tunnel registered for interface '{interface}'"
                ))
            })?;

        Ok(ExitInfo {
            ip: synthetic_ip(&tunnel.id),
            country_code: tunnel.country_code,
        })
    }
}

/// Map a tunnel id to a stable address in the TEST-NET-2 documentation range
/// (`198.51.100.0/24`), last octet held to `1..=254` so it never lands on the
/// network or broadcast address.
fn synthetic_ip(tunnel_id: &uuid::Uuid) -> String {
    let last_octet = (u32::from(tunnel_id.as_bytes()[15]) % 254) + 1;
    format!("198.51.100.{last_octet}")
}
