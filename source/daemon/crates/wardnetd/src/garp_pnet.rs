//! Real [`GarpOps`] implementation backed by `pnet`.
//!
//! On graceful shutdown the daemon needs to re-point the LAN's
//! `dhcp_router_ip` ARP entry at the upstream router's MAC; on
//! startup it re-points the same IP back at the Pi's LAN-iface MAC.
//! Both phases are 2-pulse / 500ms gratuitous ARP-reply broadcasts —
//! the sequence is part of the trait contract, not the caller's
//! problem.
//!
//! Frame layout per RFC 826 / 5227: both Ethernet src-MAC and ARP
//! sender-HW carry the phase MAC. Clients use the ARP sender field
//! to update their cache; L2 switches use Ethernet src-MAC to update
//! CAM tables. Both must agree for the failover to actually move
//! traffic. See issue #213, decision 4.
//!
//! Runs inside `spawn_blocking` because pnet's datalink channel is
//! synchronous, mirroring `pnet_network_probe.rs`. The 500ms
//! inter-pulse gap is `std::thread::sleep` *inside* the blocking
//! job — the entire phase is one self-contained synchronous unit.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use pnet::datalink::{self, Channel, Config};
use pnet::packet::arp::{ArpHardwareTypes, ArpOperations, MutableArpPacket};
use pnet::packet::ethernet::{EtherTypes, MutableEthernetPacket};
use pnet::util::MacAddr;
use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_services::garp::GarpOps;

use crate::packet_capture_pnet::find_interface;

/// 500ms between the two GARP pulses, per RFC 5227 §2.4 (gratuitous
/// announcements). Long enough to dodge transient drops, short enough
/// that the full phase (2 pulses) completes inside the 1s budget.
pub(crate) const PULSE_GAP: Duration = Duration::from_millis(500);

/// Number of GARP pulses per phase. RFC 5227 recommends 2-3
/// announcements; 2 keeps the total phase ≤ 1s while still tolerating
/// a single dropped frame.
pub(crate) const PULSE_COUNT: usize = 2;

/// Real [`GarpOps`] using `pnet`'s datalink channel.
pub struct PnetGarpOps {
    lan_interface: String,
    system_config: Arc<dyn SystemConfigRepository>,
}

impl PnetGarpOps {
    #[must_use]
    pub fn new(lan_interface: String, system_config: Arc<dyn SystemConfigRepository>) -> Self {
        Self {
            lan_interface,
            system_config,
        }
    }
}

#[async_trait]
impl GarpOps for PnetGarpOps {
    async fn broadcast_farewell(&self) -> anyhow::Result<()> {
        let Some(router_ip) = read_router_ip(&self.system_config).await? else {
            tracing::info!("GARP farewell skipped: dhcp_router_ip unset");
            return Ok(());
        };

        let Some(router_mac) = read_router_mac(&self.system_config).await? else {
            tracing::warn!(
                router_ip = %router_ip,
                "GARP farewell skipped: garp_router_mac unset (discovery has not seen the upstream router yet)",
            );
            return Ok(());
        };

        let interface = self.lan_interface.clone();
        tokio::task::spawn_blocking(move || {
            run_farewell_sequence(&interface, router_mac, router_ip)
        })
        .await
        .map_err(|e| anyhow::anyhow!("garp farewell blocking task panicked: {e}"))?
    }

    async fn broadcast_claim(&self) -> anyhow::Result<()> {
        let Some(router_ip) = read_router_ip(&self.system_config).await? else {
            tracing::info!("GARP claim skipped: dhcp_router_ip unset");
            return Ok(());
        };

        let interface = self.lan_interface.clone();
        tokio::task::spawn_blocking(move || run_claim_sequence(&interface, router_ip))
            .await
            .map_err(|e| anyhow::anyhow!("garp claim blocking task panicked: {e}"))?
    }
}

/// Read `dhcp_router_ip` and parse it. Returns `None` for unset or empty.
async fn read_router_ip(
    repo: &Arc<dyn SystemConfigRepository>,
) -> anyhow::Result<Option<Ipv4Addr>> {
    let Some(raw) = repo.get("dhcp_router_ip").await? else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let ip: Ipv4Addr = trimmed
        .parse()
        .map_err(|e| anyhow::anyhow!("dhcp_router_ip is not a valid IPv4 address ({raw}): {e}"))?;
    Ok(Some(ip))
}

/// Read `garp_router_mac` and parse it. Returns `None` for unset or empty.
async fn read_router_mac(
    repo: &Arc<dyn SystemConfigRepository>,
) -> anyhow::Result<Option<MacAddr>> {
    let Some(raw) = repo.get("garp_router_mac").await? else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mac: MacAddr = trimmed
        .parse()
        .map_err(|e| anyhow::anyhow!("garp_router_mac is not a valid MAC ({raw}): {e}"))?;
    Ok(Some(mac))
}

/// Synchronous farewell sequence. Resolves the interface and emits
/// the 2-pulse GARP burst from the upstream router's MAC.
fn run_farewell_sequence(
    interface_name: &str,
    sender_mac: MacAddr,
    sender_ip: Ipv4Addr,
) -> anyhow::Result<()> {
    let iface = find_interface(interface_name)?;
    emit_garp_burst(&iface, sender_mac, sender_ip, "farewell")
}

/// Synchronous claim sequence. The Pi's LAN-iface MAC isn't in
/// `system_config` — we resolve it from the live interface. That
/// also means the "no MAC on the interface" no-op lives here.
fn run_claim_sequence(interface_name: &str, sender_ip: Ipv4Addr) -> anyhow::Result<()> {
    let iface = find_interface(interface_name)?;
    let Some(sender_mac) = iface.mac else {
        tracing::warn!(
            interface = %interface_name,
            "GARP claim skipped: interface has no MAC address",
        );
        return Ok(());
    };
    emit_garp_burst(&iface, sender_mac, sender_ip, "claim")
}

/// Emit the 2-pulse / 500ms GARP burst on `iface`. `phase` is a short
/// label (`"claim"` / `"farewell"`) used in log lines so operators can
/// tell which sequence emitted a given pulse without diffing log
/// formats. Errors at the channel-open level surface; per-pulse send
/// failures log a warning and continue — one dropped pulse out of two
/// is still useful.
fn emit_garp_burst(
    iface: &pnet::datalink::NetworkInterface,
    sender_mac: MacAddr,
    sender_ip: Ipv4Addr,
    phase: &str,
) -> anyhow::Result<()> {
    let Channel::Ethernet(mut tx, _rx) = datalink::channel(iface, Config::default())? else {
        anyhow::bail!("unsupported channel type for interface '{}'", iface.name);
    };

    let frame = build_garp_reply(sender_mac, sender_ip)
        .ok_or_else(|| anyhow::anyhow!("failed to build GARP reply frame"))?;

    for pulse in 0..PULSE_COUNT {
        match tx.send_to(&frame, None) {
            Some(Ok(())) => {
                tracing::debug!(
                    phase = phase,
                    pulse = pulse + 1,
                    of = PULSE_COUNT,
                    sender_mac = %sender_mac,
                    sender_ip = %sender_ip,
                    "sent GARP pulse",
                );
            }
            Some(Err(e)) => {
                tracing::warn!(
                    phase = phase,
                    pulse = pulse + 1,
                    error = %e,
                    "GARP pulse send failed",
                );
            }
            None => {
                tracing::warn!(
                    phase = phase,
                    pulse = pulse + 1,
                    "datalink channel rejected GARP pulse",
                );
            }
        }
        if pulse + 1 < PULSE_COUNT {
            std::thread::sleep(PULSE_GAP);
        }
    }

    Ok(())
}

/// Build a 42-byte gratuitous ARP-reply Ethernet frame.
///
/// - Ethernet dest = `ff:ff:ff:ff:ff:ff` (broadcast) so every host
///   on the LAN sees it.
/// - Ethernet src = `sender_mac` so L2 switches refresh their CAM
///   table to point the right port at this MAC.
/// - ARP opcode = 2 (reply) — gratuitous announcements are typically
///   replies to nobody, since clients update on reply more reliably
///   than on request.
/// - sender HW / sender proto = `(sender_mac, sender_ip)` — the pair
///   the receiver is meant to install.
/// - target HW = broadcast, target proto = `sender_ip` — RFC 5227
///   form (target proto = sender proto), and target HW broadcast is
///   the safe choice for an unsolicited reply.
#[must_use]
pub fn build_garp_reply(sender_mac: MacAddr, sender_ip: Ipv4Addr) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 42];

    {
        let mut eth = MutableEthernetPacket::new(&mut buf)?;
        eth.set_destination(MacAddr::broadcast());
        eth.set_source(sender_mac);
        eth.set_ethertype(EtherTypes::Arp);
    }

    {
        let mut arp = MutableArpPacket::new(&mut buf[14..])?;
        arp.set_hardware_type(ArpHardwareTypes::Ethernet);
        arp.set_protocol_type(EtherTypes::Ipv4);
        arp.set_hw_addr_len(6);
        arp.set_proto_addr_len(4);
        arp.set_operation(ArpOperations::Reply);
        arp.set_sender_hw_addr(sender_mac);
        arp.set_sender_proto_addr(sender_ip);
        arp.set_target_hw_addr(MacAddr::broadcast());
        arp.set_target_proto_addr(sender_ip);
    }

    Some(buf)
}

