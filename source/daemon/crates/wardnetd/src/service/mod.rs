pub mod auth;
pub mod device;
pub mod dhcp;
pub mod discovery;
pub mod dns;
pub mod provider;
pub mod routing;
pub mod system;
pub mod tunnel;

pub use auth::{AuthService, AuthServiceImpl};
pub use device::{DeviceService, DeviceServiceImpl};
pub use dhcp::{DhcpService, DhcpServiceImpl};
pub use discovery::{DeviceDiscoveryService, DeviceDiscoveryServiceImpl, ObservationResult};
pub use dns::{DnsService, DnsServiceImpl};
pub use provider::{ProviderService, ProviderServiceImpl};
pub use routing::{RoutingService, RoutingServiceImpl};
pub use system::{SystemService, SystemServiceImpl};
pub use tunnel::{TunnelService, TunnelServiceImpl};

#[cfg(test)]
mod tests;
