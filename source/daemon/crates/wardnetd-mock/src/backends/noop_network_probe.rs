//! No-op [`NetworkProbe`] for the mock server.
//!
//! `arp_probe` returns a deterministic synthetic MAC so the wizard's
//! router-MAC step produces a value without sending raw packets
//! (which wouldn't work on macOS anyway). The real ARP probe lives
//! in `wardnetd::system::ArpNetworkProbe`.

use std::net::Ipv4Addr;

use async_trait::async_trait;
use wardnetd_services::system::{DhcpProbeOutcome, NetworkProbe};

#[derive(Debug, Default, Clone)]
pub struct NoopNetworkProbe;

#[async_trait]
impl NetworkProbe for NoopNetworkProbe {
    async fn arp_probe(&self, _target_ip: Ipv4Addr) -> anyhow::Result<Option<String>> {
        Ok(Some("DE:AD:BE:EF:00:01".to_owned()))
    }

    async fn dhcp_self_probe(&self) -> anyhow::Result<DhcpProbeOutcome> {
        // Synthetic "Wardnet owns DHCP and nobody else" outcome —
        // matches the synthetic LAN IP the noop inspector reports
        // (192.168.1.1) so the wizard's primary-mode happy path
        // surfaces the green tick without further config.
        Ok(DhcpProbeOutcome {
            responders: vec![Ipv4Addr::new(192, 168, 1, 1)],
        })
    }
}
