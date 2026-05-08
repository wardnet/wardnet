use std::net::Ipv4Addr;

use async_trait::async_trait;
use wardnet_common::api::DhcpSource;

/// Snapshot of the LAN interface's current state.
///
/// Built by [`NetworkInspector`] implementations (real Linux impl in
/// `wardnetd`, no-op stand-in in `wardnetd-mock`) and converted into
/// [`wardnet_common::api::NetworkStatusResponse`] by `SystemService`.
#[derive(Debug, Clone)]
pub struct NetworkSnapshot {
    pub interface: String,
    pub ip: Ipv4Addr,
    pub gateway: Option<Ipv4Addr>,
    pub dhcp_source: DhcpSource,
}

/// Reads the LAN interface's current address + default gateway and
/// classifies whether the IP came from DHCP or a Wardnet-managed
/// static configuration.
///
/// Pure read trait — no kernel mutations. The wizard's network step
/// uses this to decide whether to surface a "your IP is still
/// DHCP-derived" remediation panel.
#[async_trait]
pub trait NetworkInspector: Send + Sync {
    async fn inspect(&self) -> anyhow::Result<NetworkSnapshot>;
}
