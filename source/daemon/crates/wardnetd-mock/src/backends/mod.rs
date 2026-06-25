//! No-op network backend implementations for the mock server.
//!
//! Every backend implements the relevant trait from `wardnetd_services` and
//! performs **no** kernel or network I/O. Methods log their invocation at
//! `debug` level (so web-ui devs can see activity) and return `Ok(())` or a
//! sensible default value.
//!
//! Filenames are intentionally prefixed `noop_` to match the workspace
//! coverage-ignore regex (see top-level `Makefile`).

pub mod noop_cert_activator;
pub mod noop_device;
pub mod noop_dhcp;
pub mod noop_dns;
pub mod noop_exit_probe;
pub mod noop_garp;
pub mod noop_latency_prober;
pub mod noop_network_inspector;
pub mod noop_network_probe;
pub mod noop_power_ops;
pub mod noop_remote_access;
pub mod noop_routing;
pub mod noop_tunnel;
pub mod noop_watchdog;
