// Real backend implementations (Linux-specific).
pub mod command;
pub mod firewall_nftables;
pub mod hostname_resolver;
pub mod packet_capture_pnet;
pub mod policy_router_netlink;
pub mod tunnel_interface_wireguard;

// DHCP/DNS server implementations.
pub mod dhcp;
pub mod dns;

// Background tasks.
pub mod device_detector;
pub mod metrics_collector;
pub mod profiling;
pub mod route_monitor;
pub mod routing_listener;
pub mod tunnel_idle;
pub mod tunnel_monitor;

#[cfg(test)]
mod tests;
