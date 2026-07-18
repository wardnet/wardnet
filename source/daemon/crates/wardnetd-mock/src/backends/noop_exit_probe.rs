//! Deterministic [`TunnelExitProbe`] implementation for the mock server.
//!
//! Looks up the tunnel by interface name in the in-memory tunnel repo
//! and returns a synthetic exit IP plus the tunnel's stored country
//! code, so a developer running `make run-dev` can exercise the
//! tunnel-test flow without any real network egress.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use wardnetd_data::repository::TunnelRepository;
use wardnetd_services::tunnel::exit_probe::{ExitInfo, ProbeError, TunnelExitProbe};

/// Synthetic-IP / country probe for the mock daemon.
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
        // 75 ms to mimic a real DC handshake + probe round trip without
        // making `make run-dev` feel sluggish.
        tokio::time::sleep(Duration::from_millis(75)).await;

        let tunnels = self
            .tunnels
            .find_all()
            .await
            .map_err(|e| ProbeError::Connect(format!("mock probe repo lookup failed: {e}")))?;
        let tunnel = tunnels
            .into_iter()
            .find(|t| t.interface_name == interface)
            .ok_or_else(|| {
                ProbeError::Connect(format!(
                    "mock probe: no tunnel registered for interface '{interface}'"
                ))
            })?;

        Ok(ExitInfo {
            ip: synthetic_ip(&tunnel.id),
            country_code: tunnel.country_code,
        })
    }
}

/// Map a tunnel id to a stable address inside the TEST-NET-2 documentation
/// range (`198.51.100.0/24`). The `198.51.100.<n>` last octet stays in
/// `1..=254` so it never collides with `.0` (network) or `.255`
/// (broadcast).
pub(crate) fn synthetic_ip(tunnel_id: &uuid::Uuid) -> String {
    let bytes = tunnel_id.as_bytes();
    let last_octet = (u32::from(bytes[15]) % 254) + 1;
    format!("198.51.100.{last_octet}")
}
