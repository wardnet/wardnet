// PID file management.
pub mod pidfile;

// Real backend implementations (Linux-specific).
pub mod command;
pub mod firewall_netlink;
pub mod garp_pnet;
pub mod hostname_resolver;
pub mod inbound_wg_interface_wireguard;
pub mod packet_capture_pnet;
pub mod policy_router_netlink;
mod reqwest_client;
// Deterministic no-op tunnel backends, wired only under `[test]` config.
pub mod noop_tunnel_backends;
pub mod tunnel_exit_probe;
pub mod tunnel_interface_wireguard;
pub mod tunnel_latency_prober;
pub mod tunnel_throughput_tester;
pub mod wireguard_interface;

// DHCP/DNS server implementations.
pub mod dhcp;
pub mod dns;

// Host-power operations (systemctl reboot/poweroff).
pub mod system;

// Shutdown-cause classification and teardown of the kernel state the daemon
// created (nftables table, wireguard interfaces) — see issue #864.
pub mod shutdown;

// `wardnetd uninstall` — removes the daemon, its host state, and (with
// --purge) its data. Lives here because only the binary can delete the
// nftables table via netlink; see ADR 0013 and issue #864.
pub mod uninstall;

// Daemon-owned TLS termination: :443 RustlsConfig + hot cert reload, :80→:443
// redirect, and the 503 "not provisioned" guard.
pub mod tls_server;

// Background tasks.
pub mod access_request_listener;
pub mod device_detector;
pub mod device_snapshot_listener;
pub mod dns_device_snapshot_listener;
pub mod entitlement_listener;
pub mod garp_learning;
pub mod health_runner;
pub mod heartbeat;
pub mod inbound_wg_peer_monitor;
pub mod mdns_advertiser;
pub mod mdns_observer;
pub mod metrics_collector;
pub mod profiling;
pub mod route_monitor;
pub mod routing_listener;
pub mod tunnel_idle;
pub mod tunnel_monitor;
pub mod zone_enforcement_listener;

// Three-layer watchdog (issue #214): the health-gated soft sd_notify restart
// and the ungated hardware /dev/watchdog reboot backstop.
pub mod watchdog;

#[cfg(test)]
mod tests;
