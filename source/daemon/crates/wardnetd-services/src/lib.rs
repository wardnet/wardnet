pub mod auth_context;
pub mod command;
pub mod error;
pub mod event;
pub mod keys;
pub mod request_context;
pub mod version;

pub mod auth;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod logging;
pub mod routing;
pub mod system;
pub mod tunnel;
pub mod vpn;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Instant;

use wardnet_common::config::ApplicationConfiguration;
use wardnetd_data::RepositoryFactory;
use wardnetd_data::repository::DnsRepository;

use crate::auth::AuthServiceImpl;
use crate::device::DeviceServiceImpl;
use crate::device::discovery::DeviceDiscoveryServiceImpl;
use crate::dhcp::DhcpServiceImpl;
use crate::dns::DnsServiceImpl;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::routing::RoutingServiceImpl;
use crate::system::SystemServiceImpl;
use crate::tunnel::TunnelServiceImpl;
use crate::vpn::{VpnProviderRegistry, VpnProviderServiceImpl};

pub use crate::auth::AuthService;
pub use crate::device::{DeviceDiscoveryService, DeviceService, ObservationResult};
pub use crate::dhcp::DhcpService;
pub use crate::dns::DnsService;
pub use crate::logging::LogService;
pub use crate::routing::RoutingService;
pub use crate::system::SystemService;
pub use crate::tunnel::TunnelService;
pub use crate::vpn::VpnProviderService;

/// Backends provided by the caller (real or mock).
///
/// The daemon passes real implementations (`WireGuard`, nftables, etc.);
/// the mock server passes no-op stubs.
pub struct Backends {
    pub tunnel_interface: Arc<dyn tunnel::TunnelInterface>,
    pub policy_router: Arc<dyn routing::PolicyRouter>,
    pub firewall: Arc<dyn routing::FirewallManager>,
    pub packet_capture: Arc<dyn device::PacketCapture>,
    pub hostname_resolver: Arc<dyn device::HostnameResolver>,
    pub key_store: Arc<dyn wardnetd_data::keys::KeyStore>,
}

/// All wired services, ready to use.
pub struct Services {
    pub auth: Arc<dyn AuthService>,
    pub device: Arc<dyn DeviceService>,
    pub dhcp: Arc<dyn DhcpService>,
    pub dns: Arc<dyn DnsService>,
    pub discovery: Arc<dyn DeviceDiscoveryService>,
    pub log: Arc<dyn LogService>,
    pub vpn_provider: Arc<dyn VpnProviderService>,
    pub routing: Arc<dyn RoutingService>,
    pub system: Arc<dyn SystemService>,
    pub tunnel: Arc<dyn TunnelService>,
    pub event_publisher: Arc<dyn EventPublisher>,
    pub dns_repo: Arc<dyn DnsRepository>,
}

/// Initialize all services from the application configuration.
///
/// This is the main entry point for both the daemon and the mock server.
/// It handles:
/// 1. Database pool initialization via [`wardnetd_data::create_repository_factory`]
/// 2. Admin account bootstrap
/// 3. Service wiring with the provided [`Backends`]
///
/// The caller provides the [`Backends`] (real or mock) and the LAN IP
/// (detected by the daemon, hardcoded by the mock).
pub async fn init_services(
    config: &ApplicationConfiguration,
    backends: Backends,
    lan_ip: std::net::Ipv4Addr,
    started_at: Instant,
    log_service: Arc<dyn LogService>,
) -> anyhow::Result<Services> {
    // Initialize the repository factory from config (validates provider).
    let repo_factory = wardnetd_data::create_repository_factory(config).await?;

    // Bootstrap admin account.
    let admin_credentials = config
        .admin
        .as_ref()
        .map(|a| (a.username.as_str(), a.password.as_str()));
    wardnetd_data::bootstrap::bootstrap_admin(&repo_factory.admin(), admin_credentials).await?;

    // Wire services.
    Ok(create_services(
        repo_factory.as_ref(),
        backends,
        config,
        lan_ip,
        started_at,
        log_service,
    ))
}

/// Initialize all services using a pre-built [`RepositoryFactory`].
///
/// Variant of [`init_services`] for callers that need to interact with the
/// factory *before* services are wired — for example, the mock server seeds
/// demo data directly through repositories, then calls this helper to build
/// the service layer on top of the already-populated database.
///
/// Unlike [`init_services`], this function does **not** bootstrap an admin
/// account: the caller is expected to control that lifecycle (e.g. the mock
/// server intentionally leaves the DB without an admin so the setup wizard
/// runs on every launch).
pub fn init_services_with_factory(
    repo_factory: &dyn RepositoryFactory,
    backends: Backends,
    config: &ApplicationConfiguration,
    lan_ip: std::net::Ipv4Addr,
    started_at: Instant,
    log_service: Arc<dyn LogService>,
) -> Services {
    create_services(
        repo_factory,
        backends,
        config,
        lan_ip,
        started_at,
        log_service,
    )
}

/// Creates services from a [`RepositoryFactory`] + [`Backends`] + config.
///
/// Lower-level than [`init_services`] — used when the caller already has
/// a repository factory (e.g. in tests).
#[must_use]
fn create_services(
    repo_factory: &dyn RepositoryFactory,
    backends: Backends,
    config: &ApplicationConfiguration,
    lan_ip: std::net::Ipv4Addr,
    started_at: Instant,
    log_service: Arc<dyn LogService>,
) -> Services {
    let admin_repo = repo_factory.admin();
    let session_repo = repo_factory.session();
    let api_key_repo = repo_factory.api_key();
    let device_repo = repo_factory.device();
    let system_config_repo = repo_factory.system_config();
    let dhcp_repo = repo_factory.dhcp();
    let dns_repo = repo_factory.dns();
    let tunnel_repo = repo_factory.tunnel();

    let event_publisher: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(256));

    let auth_service: Arc<dyn AuthService> = Arc::new(AuthServiceImpl::new(
        admin_repo,
        session_repo,
        api_key_repo,
        system_config_repo.clone(),
        config.auth.session_expiry_hours,
    ));

    let device_service: Arc<dyn DeviceService> = Arc::new(DeviceServiceImpl::new(
        device_repo.clone(),
        event_publisher.clone(),
    ));

    let dhcp_service: Arc<dyn DhcpService> = Arc::new(DhcpServiceImpl::new(
        dhcp_repo,
        system_config_repo.clone(),
        lan_ip,
    ));

    let dns_service: Arc<dyn DnsService> = Arc::new(DnsServiceImpl::new(
        system_config_repo.clone(),
        dns_repo.clone(),
        event_publisher.clone(),
    ));

    let system_service: Arc<dyn SystemService> = Arc::new(SystemServiceImpl::new(
        system_config_repo,
        tunnel_repo.clone(),
        started_at,
    ));

    let tunnel_service: Arc<dyn TunnelService> = Arc::new(TunnelServiceImpl::new(
        tunnel_repo.clone(),
        device_repo.clone(),
        backends.tunnel_interface.clone(),
        backends.key_store,
        event_publisher.clone(),
    ));

    let registry = Arc::new(VpnProviderRegistry::new(&config.vpn_providers.enabled));
    let vpn_provider_service: Arc<dyn VpnProviderService> = Arc::new(VpnProviderServiceImpl::new(
        registry,
        tunnel_service.clone(),
    ));

    let lan_subnet = ipnetwork::Ipv4Network::new(lan_ip, 24).unwrap_or_else(|_| {
        tracing::warn!("failed to create LAN subnet, using /24 default");
        ipnetwork::Ipv4Network::new(lan_ip, 24).expect("valid /24")
    });

    let discovery_service: Arc<dyn DeviceDiscoveryService> =
        Arc::new(DeviceDiscoveryServiceImpl::new(
            device_repo.clone(),
            event_publisher.clone(),
            backends.hostname_resolver,
            lan_subnet,
        ));

    let routing_service: Arc<dyn RoutingService> = Arc::new(RoutingServiceImpl::new(
        device_repo,
        tunnel_repo,
        tunnel_service.clone(),
        backends.policy_router,
        backends.firewall,
        config.network.default_policy.clone(),
        config.network.lan_interface.clone(),
    ));

    Services {
        auth: auth_service,
        device: device_service,
        dhcp: dhcp_service,
        dns: dns_service,
        log: log_service,
        discovery: discovery_service,
        vpn_provider: vpn_provider_service,
        routing: routing_service,
        system: system_service,
        tunnel: tunnel_service,
        event_publisher,
        dns_repo,
    }
}
