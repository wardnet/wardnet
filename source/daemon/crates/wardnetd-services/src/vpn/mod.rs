pub mod nordvpn;
pub mod provider;
pub mod registry;
pub mod service;

pub use provider::VpnProvider;
pub use registry::{EnabledProviders, VpnProviderRegistry};
pub use service::{VpnProviderService, VpnProviderServiceImpl};

#[cfg(test)]
mod tests;
