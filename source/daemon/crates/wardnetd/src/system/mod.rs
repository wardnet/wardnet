//! Real (Linux) implementations of host-power operations.
//!
//! The daemon shells out via the existing [`CommandExecutor`] so the
//! same `/usr/sbin` resolution rules that apply to `ip`, `nft`, and
//! `sysctl` apply here too.
//!
//! [`CommandExecutor`]: wardnetd_services::command::CommandExecutor

pub mod proc_net_inspector;
pub mod systemctl_power_ops;

pub use proc_net_inspector::ProcNetNetworkInspector;
pub use systemctl_power_ops::SystemctlPowerOps;
