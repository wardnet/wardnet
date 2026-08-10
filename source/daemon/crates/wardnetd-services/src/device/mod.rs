pub mod discovery;
pub mod hostname_resolver;
pub mod identification;
pub mod ip_snapshot;
pub mod packet_capture;
pub mod retention_runner;
pub mod service;

pub use discovery::{
    DEVICE_RETENTION_DAYS, DeviceDiscoveryService, DeviceDiscoveryServiceImpl, ObservationResult,
};
pub use hostname_resolver::HostnameResolver;
pub use identification::{DeviceIdentificationService, DeviceIdentificationServiceImpl};
pub use ip_snapshot::DeviceIpSnapshot;
pub use packet_capture::{ObservedDevice, PacketCapture, PacketSource};
pub use retention_runner::DeviceRetentionRunner;
pub use service::{DeviceService, DeviceServiceImpl};

#[cfg(test)]
mod tests;
