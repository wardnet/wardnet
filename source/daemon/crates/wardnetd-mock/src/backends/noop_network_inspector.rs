//! No-op [`NetworkInspector`] for the mock server.
//!
//! Returns a synthetic snapshot that matches the mock's hardcoded
//! LAN IP (192.168.1.1) so the wizard's network step renders
//! plausible data without touching `/proc/net/route` (which doesn't
//! exist on macOS and would otherwise block local dev).

use std::net::Ipv4Addr;

use async_trait::async_trait;
use wardnet_common::api::DhcpSource;
use wardnetd_services::system::{NetworkInspector, NetworkSnapshot};

#[derive(Debug, Clone)]
pub struct NoopNetworkInspector {
    pub interface: String,
    pub ip: Ipv4Addr,
}

#[async_trait]
impl NetworkInspector for NoopNetworkInspector {
    async fn inspect(&self) -> anyhow::Result<NetworkSnapshot> {
        Ok(NetworkSnapshot {
            interface: self.interface.clone(),
            ip: self.ip,
            // Stable synthetic gateway so the wizard always shows a
            // sensible value to the operator.
            gateway: Some(Ipv4Addr::new(192, 168, 1, 254)),
            // Surface the static branch by default so the remediation
            // panel doesn't fire on every mock launch — operators can
            // exercise the DHCP path explicitly when they need to.
            dhcp_source: DhcpSource::Static,
        })
    }
}
