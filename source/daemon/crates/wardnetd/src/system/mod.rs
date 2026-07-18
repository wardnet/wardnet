//! Real (Linux) implementations of host-power operations.
//!
//! The daemon shells out via the existing [`CommandExecutor`] so the
//! same `/usr/sbin` resolution rules that apply to `ip`, `nft`, and
//! `sysctl` apply here too.
//!
//! [`CommandExecutor`]: wardnetd_services::command::CommandExecutor

pub mod linux_watchdog;
pub mod pnet_network_probe;
pub mod proc_net_inspector;
pub mod systemctl_power_ops;

#[cfg(test)]
mod tests;

pub use linux_watchdog::LinuxWatchdog;
pub use pnet_network_probe::PnetNetworkProbe;
pub use proc_net_inspector::ProcNetNetworkInspector;
pub use systemctl_power_ops::SystemctlPowerOps;
