pub mod network_inspector;
pub mod network_probe;
pub mod power_ops;
pub mod service;
pub mod watchdog_ops;

pub use network_inspector::{NetworkInspector, NetworkSnapshot};
pub use network_probe::{DhcpProbeOutcome, NetworkProbe};
pub use power_ops::SystemPowerOps;
pub use service::{SystemService, SystemServiceImpl};
pub use watchdog_ops::WatchdogOps;

#[cfg(test)]
mod tests;
