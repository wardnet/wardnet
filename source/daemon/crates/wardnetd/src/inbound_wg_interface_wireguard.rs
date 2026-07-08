use async_trait::async_trait;
use wireguard_control::{Backend, Device, DeviceUpdate, InterfaceName, Key, PeerConfigBuilder};

use wardnetd_services::inbound_wg::interface::{
    InboundWgInterface, InboundWgPeerConfig, InboundWgPeerStats, InboundWgServerConfig,
};

/// Build a single peer's [`InboundWgPeerStats`] from the raw fields read off a
/// `wireguard_control` peer. Split out as a pure function so the mapping (and
/// the `SystemTime` → `DateTime<Utc>` conversion) is unit-testable without a
/// kernel interface.
#[must_use]
pub fn peer_stats_from(
    public_key: [u8; 32],
    tx_bytes: u64,
    rx_bytes: u64,
    last_handshake_time: Option<std::time::SystemTime>,
) -> InboundWgPeerStats {
    InboundWgPeerStats {
        public_key,
        bytes_tx: tx_bytes,
        bytes_rx: rx_bytes,
        last_handshake: last_handshake_time.map(chrono::DateTime::<chrono::Utc>::from),
    }
}

/// Production [`InboundWgInterface`] backed by the `wireguard-control` crate.
///
/// Mirrors [`WireGuardTunnelInterface`](crate::tunnel_interface_wireguard::WireGuardTunnelInterface)
/// but peer-list-shaped: the server interface is stood up once and peers are
/// added/removed incrementally. Communicates via netlink on Linux.
#[derive(Debug)]
pub struct WireGuardInboundInterface;

#[async_trait]
impl InboundWgInterface for WireGuardInboundInterface {
    async fn ensure_server(&self, config: InboundWgServerConfig) -> anyhow::Result<()> {
        let iface: InterfaceName = config
            .interface_name
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid interface name: {e}"))?;

        // Remove any stale interface left over from a previous daemon run or
        // crash so re-creation doesn't hit "Address already assigned". The
        // service re-adds every enabled peer immediately after this call, so
        // dropping the interface's current peers here is safe and intentional.
        let check = tokio::process::Command::new("ip")
            .args(["link", "show", &config.interface_name])
            .output()
            .await;
        if check.is_ok_and(|o| o.status.success()) {
            tracing::info!(
                interface = %config.interface_name,
                "removing stale inbound wireguard interface before re-creation"
            );
            let _ = tokio::process::Command::new("ip")
                .args(["link", "delete", &config.interface_name])
                .output()
                .await;
        }

        // Configure the server key + listen port (this creates the interface).
        DeviceUpdate::new()
            .set_private_key(Key(config.private_key))
            .set_listen_port(config.listen_port)
            .apply(&iface, Backend::default())?;

        // Assign the server address(es) (e.g. `10.100.64.1/24`). Tolerate
        // EEXIST so re-applying is idempotent.
        for addr in &config.address {
            let output = tokio::process::Command::new("ip")
                .args([
                    "addr",
                    "add",
                    &addr.to_string(),
                    "dev",
                    &config.interface_name,
                ])
                .output()
                .await?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.contains("RTNETLINK answers: File exists")
                    && !stderr.contains("Address already assigned")
                {
                    anyhow::bail!(
                        "`ip addr add {} dev {}` failed: {}",
                        addr,
                        config.interface_name,
                        stderr.trim()
                    );
                }
            }
        }

        // Bring the interface UP so peers can route.
        let output = tokio::process::Command::new("ip")
            .args(["link", "set", &config.interface_name, "up"])
            .output()
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "`ip link set {} up` failed: {}",
                config.interface_name,
                stderr.trim()
            );
        }

        tracing::info!(interface = %config.interface_name, "inbound wireguard server ensured");
        Ok(())
    }

    async fn tear_down_server(&self, interface_name: &str) -> anyhow::Result<()> {
        // Best-effort delete: an absent interface is the expected steady state
        // when the server was never enabled, so a failure here is non-fatal.
        let output = tokio::process::Command::new("ip")
            .args(["link", "delete", interface_name])
            .output()
            .await?;
        if output.status.success() {
            tracing::info!(interface = %interface_name, "inbound wireguard server torn down");
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::debug!(
                interface = %interface_name,
                "inbound wireguard tear-down skipped (interface absent?): {}",
                stderr.trim()
            );
        }
        Ok(())
    }

    async fn add_peer(
        &self,
        interface_name: &str,
        peer: InboundWgPeerConfig,
    ) -> anyhow::Result<()> {
        let iface: InterfaceName = interface_name
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid interface name: {e}"))?;

        let mut builder = PeerConfigBuilder::new(&Key(peer.public_key));
        for network in &peer.allowed_ips {
            builder = builder.add_allowed_ip(network.ip(), network.prefix());
        }
        if let Some(psk) = peer.preshared_key {
            builder = builder.set_preshared_key(Key(psk));
        }
        if let Some(keepalive) = peer.persistent_keepalive {
            builder = builder.set_persistent_keepalive_interval(keepalive);
        }

        // `add_peer` is incremental (`replace_peers` defaults false), so this
        // never disturbs the peers already on the interface.
        DeviceUpdate::new()
            .add_peer(builder)
            .apply(&iface, Backend::default())?;

        tracing::debug!(interface = %interface_name, "inbound wireguard peer added");
        Ok(())
    }

    async fn remove_peer(&self, interface_name: &str, public_key: [u8; 32]) -> anyhow::Result<()> {
        let iface: InterfaceName = interface_name
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid interface name: {e}"))?;

        DeviceUpdate::new()
            .remove_peer_by_key(&Key(public_key))
            .apply(&iface, Backend::default())?;

        tracing::debug!(interface = %interface_name, "inbound wireguard peer removed");
        Ok(())
    }

    async fn peer_stats(&self, interface_name: &str) -> anyhow::Result<Vec<InboundWgPeerStats>> {
        let iface: InterfaceName = interface_name
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid interface name: {e}"))?;

        let Ok(device) = Device::get(&iface, Backend::default()) else {
            return Ok(Vec::new());
        };

        Ok(device
            .peers
            .iter()
            .map(|p| {
                peer_stats_from(
                    p.config.public_key.0,
                    p.stats.tx_bytes,
                    p.stats.rx_bytes,
                    p.stats.last_handshake_time,
                )
            })
            .collect())
    }
}
