pub mod discovery;
pub mod hostname_resolver;
pub mod packet_capture;
pub mod service;

pub use discovery::{DeviceDiscoveryService, DeviceDiscoveryServiceImpl, ObservationResult};
pub use hostname_resolver::HostnameResolver;
pub use packet_capture::{ObservedDevice, PacketCapture, PacketSource};
pub use service::{DeviceService, DeviceServiceImpl};

#[cfg(test)]
mod tests;
