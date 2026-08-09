mod device_detector;
mod device_snapshot_listener;
mod firewall_netlink;
mod garp_learning;
mod garp_pnet;
mod health_runner;
mod heartbeat;
mod hostname_resolver;
mod inbound_wg_interface_wireguard;
mod linux_watchdog;
mod mdns_advertiser;
mod mdns_observer;
mod metrics_collector;
mod packet_capture;
mod pidfile;
mod policy_router_netlink;
mod profiling;
mod routing_listener;
pub mod stubs;
mod systemctl_power_ops;
mod tls_server;
mod tunnel_exit_probe;
mod tunnel_idle;
mod tunnel_interface_wireguard;
mod tunnel_monitor;
mod tunnel_throughput_tester;
mod watchdog_hard;
mod watchdog_soft;
mod wireguard_interface;
mod zone_enforcement_listener;

/// Apply `update` to a kernel `WireGuard` interface for an opt-in `kernel_*`
/// test, returning `false` when this kernel has no `WireGuard` support (the
/// caller should skip the test).
///
/// Panics on any other failure (e.g. missing `CAP_NET_ADMIN`).
pub fn apply_or_skip_kernel(
    update: wireguard_control::DeviceUpdate,
    iface: &wireguard_control::InterfaceName,
) -> bool {
    match update.apply(iface, wireguard_control::Backend::Kernel) {
        Ok(()) => true,
        Err(e) if e.raw_os_error() == Some(libc::EOPNOTSUPP) => {
            eprintln!("skipping: this kernel has no WireGuard support ({e})");
            false
        }
        Err(e) => {
            panic!("failed to create kernel wireguard interface (needs CAP_NET_ADMIN): {e}")
        }
    }
}
