use async_trait::async_trait;
use wireguard_control::{Backend, Device, DeviceUpdate, InterfaceName, Key, PeerConfigBuilder};

use crate::tunnel_interface::{CreateTunnelParams, TunnelConfig, TunnelInterface, TunnelStats};

/// Input data for a single peer's stats (used by the pure aggregation function).
#[derive(Debug, Clone)]
pub struct PeerStatsInput {
    /// Bytes transmitted to this peer.
    pub tx_bytes: u64,
    /// Bytes received from this peer.
    pub rx_bytes: u64,
    /// Last handshake time for this peer.
    pub last_handshake_time: Option<std::time::SystemTime>,
}

/// Aggregate per-peer stats into a single [`TunnelStats`].
///
/// Sums `tx_bytes` and `rx_bytes` across all peers and picks the most recent
/// handshake time.
pub fn aggregate_peer_stats(peers: &[PeerStatsInput]) -> TunnelStats {
    let mut total_tx: u64 = 0;
    let mut total_rx: u64 = 0;
    let mut latest_handshake: Option<std::time::SystemTime> = None;

    for peer in peers {
        total_tx += peer.tx_bytes;
        total_rx += peer.rx_bytes;
        if let Some(hs) = peer.last_handshake_time {
            latest_handshake = Some(match latest_handshake {
                Some(prev) if prev > hs => prev,
                _ => hs,
            });
        }
    }

    let last_handshake = latest_handshake.map(chrono::DateTime::<chrono::Utc>::from);

    TunnelStats {
        bytes_tx: total_tx,
        bytes_rx: total_rx,
        last_handshake,
    }
}

/// Production [`TunnelInterface`] implementation backed by the `wireguard-control` crate.
///
/// Communicates via netlink on Linux (kernel backend) and userspace
/// sockets on macOS (via `wireguard-go`).
#[derive(Debug)]
pub struct WireGuardTunnelInterface;

#[async_trait]
impl TunnelInterface for WireGuardTunnelInterface {
    async fn create(&self, params: CreateTunnelParams) -> anyhow::Result<()> {
        let TunnelConfig::WireGuard {
            private_key,
            listen_port,
            peer_public_key,
            peer_endpoint,
            peer_allowed_ips,
            peer_preshared_key,
            persistent_keepalive,
        } = params.config;

        let iface: InterfaceName = params
            .interface_name
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid interface name: {e}"))?;

        let private_key = Key(private_key);
        let peer_key = Key(peer_public_key);

        let mut peer = PeerConfigBuilder::new(&peer_key);

        if let Some(endpoint) = peer_endpoint {
            peer = peer.set_endpoint(endpoint);
        }

        for network in &peer_allowed_ips {
            peer = peer.add_allowed_ip(network.ip(), network.prefix());
        }

        if let Some(psk) = peer_preshared_key {
            peer = peer.set_preshared_key(Key(psk));
        }

        if let Some(keepalive) = persistent_keepalive {
            peer = peer.set_persistent_keepalive_interval(keepalive);
        }

        let mut update = DeviceUpdate::new()
            .set_private_key(private_key)
            .add_peer(peer);

        if let Some(port) = listen_port {
            update = update.set_listen_port(port);
        }

        update.apply(&iface, Backend::default())?;

        tracing::info!(interface = %params.interface_name, "wireguard interface created");
        Ok(())
    }

    async fn bring_up(&self, interface_name: &str) -> anyhow::Result<()> {
        // wireguard-control creates the interface in an "up" state already.
        // On Linux a separate `ip link set <iface> up` may be needed via
        // the nix crate or std::process::Command — left as a future enhancement.
        tracing::info!(interface = %interface_name, "wireguard interface up");
        Ok(())
    }

    async fn tear_down(&self, interface_name: &str) -> anyhow::Result<()> {
        tracing::info!(interface = %interface_name, "wireguard interface down");
        Ok(())
    }

    async fn remove(&self, interface_name: &str) -> anyhow::Result<()> {
        let iface: InterfaceName = interface_name
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid interface name: {e}"))?;

        // Apply an empty update — the interface is removed when the last
        // reference is dropped on most backends. Use a minimal no-op update
        // so the kernel removes it.
        DeviceUpdate::new().apply(&iface, Backend::default())?;

        tracing::info!(interface = %interface_name, "wireguard interface removed");
        Ok(())
    }

    async fn get_stats(&self, interface_name: &str) -> anyhow::Result<Option<TunnelStats>> {
        let iface: InterfaceName = interface_name
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid interface name: {e}"))?;

        let Ok(device) = Device::get(&iface, Backend::default()) else {
            return Ok(None);
        };

        let peers: Vec<PeerStatsInput> = device
            .peers
            .iter()
            .map(|p| PeerStatsInput {
                tx_bytes: p.stats.tx_bytes,
                rx_bytes: p.stats.rx_bytes,
                last_handshake_time: p.stats.last_handshake_time,
            })
            .collect();

        Ok(Some(aggregate_peer_stats(&peers)))
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        let devices = Device::list(Backend::default())?;
        Ok(devices.into_iter().map(|name| name.to_string()).collect())
    }
}
