pub mod power_ops;
pub mod service;

pub use power_ops::SystemPowerOps;
pub use service::{SystemService, SystemServiceImpl};

#[cfg(test)]
mod tests;
