use std::net::Ipv4Addr;

use async_trait::async_trait;
use pnet::datalink::{self, Channel, Config, NetworkInterface};
use pnet::packet::Packet;
use pnet::packet::arp::{ArpHardwareTypes, ArpOperations, ArpPacket, MutableArpPacket};
use pnet::packet::ethernet::{EtherTypes, EthernetPacket, MutableEthernetPacket};
use pnet::packet::ipv4::Ipv4Packet;
use pnet::util::MacAddr;
use tokio_util::sync::CancellationToken;

use crate::packet_capture::{ObservedDevice, PacketCapture, PacketSource};

/// Real packet capture implementation using `pnet`.
///
/// Passively listens for ARP and IPv4 packets on a network interface, and
/// can send ARP probes to discover all hosts on the local subnet.
#[derive(Debug)]
pub struct PnetCapture;

/// Find a network interface by name.
pub(crate) fn find_interface(name: &str) -> anyhow::Result<NetworkInterface> {
    datalink::interfaces()
        .into_iter()
        .find(|iface| iface.name == name)
        .ok_or_else(|| anyhow::anyhow!("network interface '{name}' not found"))
}

/// Get the IPv4 address of a network interface.
///
/// Tries `pnet::datalink::interfaces()` first (reads `/sys/class/net/`),
/// falls back to reading `/sys/class/net/<name>/address` + ARP table if
/// pnet returns no IPs (e.g. due to permissions).
pub fn get_interface_ipv4(name: &str) -> anyhow::Result<std::net::Ipv4Addr> {
    // Primary: pnet interface list (no special permissions needed on Linux).
    let iface = find_interface(name)?;
    if let Some(ip) = iface.ips.iter().find_map(|ip| match ip {
        pnet::ipnetwork::IpNetwork::V4(net) => Some(net.ip()),
        _ => None,
    }) {
        return Ok(ip);
    }

    // Fallback: read from /proc/net/fib_trie via interface index.
    // Parse `ip -j -4 addr show <name>` if available.
    let output = std::process::Command::new("/usr/sbin/ip")
        .args(["-j", "-4", "addr", "show", name])
        .output()
        .or_else(|_| {
            std::process::Command::new("ip")
                .args(["-j", "-4", "addr", "show", name])
                .output()
        });

    if let Ok(out) = output {
        if out.status.success() {
            // JSON output: [{"addr_info":[{"local":"10.232.1.10",...}]}]
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                if let Some(ip_str) = v
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|obj| obj.get("addr_info"))
                    .and_then(|ai| ai.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|info| info.get("local"))
                    .and_then(|l| l.as_str())
                {
                    if let Ok(ip) = ip_str.parse() {
                        return Ok(ip);
                    }
                }
            }
        }
    }

    anyhow::bail!("no IPv4 address found on interface '{name}'")
}

/// Format a `pnet` `MacAddr` as uppercase colon-separated "AA:BB:CC:DD:EE:FF".
pub(crate) fn format_mac(mac: MacAddr) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac.0, mac.1, mac.2, mac.3, mac.4, mac.5
    )
}

/// Check whether a MAC address should be filtered out.
pub(crate) fn should_filter_mac(mac: MacAddr, own_mac: MacAddr) -> bool {
    // Own MAC
    if mac == own_mac {
        return true;
    }
    // Broadcast
    if mac == MacAddr::broadcast() {
        return true;
    }
    // Multicast (bit 0 of first octet is set)
    if mac.0 & 0x01 != 0 {
        return true;
    }
    false
}

/// Parse an Ethernet frame into an `ObservedDevice`, if applicable.
pub(crate) fn parse_frame(data: &[u8], own_mac: MacAddr) -> Option<ObservedDevice> {
    let eth = EthernetPacket::new(data)?;

    match eth.get_ethertype() {
        EtherTypes::Arp => {
            let arp = ArpPacket::new(eth.payload())?;
            let sender_mac = arp.get_sender_hw_addr();
            let sender_ip = arp.get_sender_proto_addr();

            if should_filter_mac(sender_mac, own_mac) {
                return None;
            }
            // Filter out 0.0.0.0 (incomplete ARP)
            if sender_ip == Ipv4Addr::UNSPECIFIED {
                return None;
            }

            Some(ObservedDevice {
                mac: format_mac(sender_mac),
                ip: sender_ip.to_string(),
                source: PacketSource::Arp,
            })
        }
        EtherTypes::Ipv4 => {
            let src_mac = eth.get_source();
            if should_filter_mac(src_mac, own_mac) {
                return None;
            }

            let ipv4 = Ipv4Packet::new(eth.payload())?;
            let src_ip = ipv4.get_source();

            // Filter out 0.0.0.0
            if src_ip == Ipv4Addr::UNSPECIFIED {
                return None;
            }

            Some(ObservedDevice {
                mac: format_mac(src_mac),
                ip: src_ip.to_string(),
                source: PacketSource::Ip,
            })
        }
        _ => None,
    }
}

/// Build an ARP request Ethernet frame.
///
/// Returns a `Vec<u8>` containing the complete Ethernet frame.
pub(crate) fn build_arp_request(
    src_mac: MacAddr,
    src_ip: Ipv4Addr,
    target_ip: Ipv4Addr,
) -> Option<Vec<u8>> {
    // Ethernet header (14) + ARP payload (28) = 42 bytes
    let mut buf = vec![0u8; 42];

    {
        let mut eth = MutableEthernetPacket::new(&mut buf)?;
        eth.set_destination(MacAddr::broadcast());
        eth.set_source(src_mac);
        eth.set_ethertype(EtherTypes::Arp);
    }

    {
        let mut arp = MutableArpPacket::new(&mut buf[14..])?;
        arp.set_hardware_type(ArpHardwareTypes::Ethernet);
        arp.set_protocol_type(EtherTypes::Ipv4);
        arp.set_hw_addr_len(6);
        arp.set_proto_addr_len(4);
        arp.set_operation(ArpOperations::Request);
        arp.set_sender_hw_addr(src_mac);
        arp.set_sender_proto_addr(src_ip);
        arp.set_target_hw_addr(MacAddr::zero());
        arp.set_target_proto_addr(target_ip);
    }

    Some(buf)
}

/// Compute all host IPs in a subnet given an address and netmask.
///
/// Excludes network and broadcast addresses.
pub(crate) fn subnet_hosts(ip: Ipv4Addr, mask: Ipv4Addr) -> Vec<Ipv4Addr> {
    let ip_u32 = u32::from(ip);
    let mask_u32 = u32::from(mask);
    let network = ip_u32 & mask_u32;
    let broadcast = network | !mask_u32;

    // Skip network address and broadcast address
    let start = network.wrapping_add(1);
    let end = broadcast.wrapping_sub(1);

    if start > end {
        return Vec::new();
    }

    (start..=end).map(Ipv4Addr::from).collect()
}

#[async_trait]
impl PacketCapture for PnetCapture {
    async fn capture_loop(
        &self,
        interface: &str,
        sender: tokio::sync::mpsc::Sender<ObservedDevice>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let iface = find_interface(interface)?;
        let own_mac = iface
            .mac
            .ok_or_else(|| anyhow::anyhow!("interface '{interface}' has no MAC address"))?;

        let config = Config {
            read_timeout: Some(std::time::Duration::from_millis(500)),
            ..Config::default()
        };

        let Channel::Ethernet(_tx, mut rx) = datalink::channel(&iface, config)? else {
            anyhow::bail!("unsupported channel type for interface '{interface}'");
        };

        let iface_name = interface.to_owned();
        tracing::info!(interface = %iface_name, "packet capture started: interface={iface_name}");

        // Bridge blocking pnet receive loop to async world via spawn_blocking
        tokio::task::spawn_blocking(move || {
            while !cancel.is_cancelled() {
                match rx.next() {
                    Ok(data) => {
                        if let Some(obs) = parse_frame(data, own_mac) {
                            // Non-blocking send; drop if channel is full
                            let _ = sender.try_send(obs);
                        }
                    }
                    Err(e) => {
                        // Timeout errors are expected due to read_timeout config
                        if e.kind() != std::io::ErrorKind::TimedOut {
                            tracing::warn!(
                                interface = %iface_name,
                                error = %e,
                                "packet capture read error on {iface_name}: {e}"
                            );
                        }
                    }
                }
            }
            tracing::info!(interface = %iface_name, "packet capture stopped: interface={iface_name}");
        })
        .await?;

        Ok(())
    }

    async fn arp_scan(&self, interface: &str) -> anyhow::Result<()> {
        let iface = find_interface(interface)?;
        let own_mac = iface
            .mac
            .ok_or_else(|| anyhow::anyhow!("interface '{interface}' has no MAC address"))?;

        // Find the interface's IPv4 address and netmask.
        let ipv4_net = iface
            .ips
            .iter()
            .find_map(|net| match net {
                ipnetwork::IpNetwork::V4(v4) => Some(*v4),
                ipnetwork::IpNetwork::V6(_) => None,
            })
            .ok_or_else(|| anyhow::anyhow!("interface '{interface}' has no IPv4 address"))?;

        let src_ip = ipv4_net.ip();
        let mask = ipv4_net.mask();
        let hosts = subnet_hosts(src_ip, mask);

        let host_count = hosts.len();
        tracing::info!(
            interface,
            host_count,
            subnet = %ipv4_net,
            "starting ARP scan: interface={interface}, host_count={host_count}, subnet={ipv4_net}"
        );

        let config = Config::default();
        let Channel::Ethernet(mut tx, _rx) = datalink::channel(&iface, config)? else {
            anyhow::bail!("unsupported channel type for interface '{interface}'");
        };

        let iface_name = interface.to_owned();
        tokio::task::spawn_blocking(move || {
            for target_ip in hosts {
                if let Some(frame) = build_arp_request(own_mac, src_ip, target_ip)
                    && let Some(Err(e)) = tx.send_to(&frame, None)
                {
                    tracing::warn!(
                        interface = %iface_name,
                        target = %target_ip,
                        error = %e,
                        "failed to send ARP request on {iface_name} to {target_ip}: {e}"
                    );
                }
                // Small delay to avoid flooding the network
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            tracing::info!(interface = %iface_name, "ARP scan complete: interface={iface_name}");
        })
        .await?;

        Ok(())
    }
}
