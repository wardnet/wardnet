// PID file management.
pub mod pidfile;

// Real backend implementations (Linux-specific).
pub mod command;
pub mod firewall_nftables;
pub mod garp_pnet;
pub mod hostname_resolver;
pub mod packet_capture_pnet;
pub mod policy_router_netlink;
pub mod tunnel_exit_probe;
pub mod tunnel_interface_wireguard;
pub mod tunnel_latency_prober;

// DHCP/DNS server implementations.
pub mod dhcp;
pub mod dns;

// Host-power operations (systemctl reboot/poweroff).
pub mod system;

// Daemon-owned TLS termination: :443 RustlsConfig + hot cert reload, :80→:443
// redirect, and the 503 "not provisioned" guard.
pub mod tls_server;

// Background tasks.
pub mod device_detector;
pub mod garp_learning;
pub mod heartbeat;
pub mod mdns_advertiser;
pub mod metrics_collector;
pub mod profiling;
pub mod route_monitor;
pub mod routing_listener;
pub mod tunnel_idle;
pub mod tunnel_monitor;

#[cfg(test)]
mod tests;
