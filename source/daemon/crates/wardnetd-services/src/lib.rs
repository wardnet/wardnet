pub mod auth_context;
pub mod command;
pub mod error;
pub mod event;
pub mod jobs;
pub mod request_context;
pub mod secret_store;
pub mod version;

pub mod auth;
pub mod backup;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod logging;
pub mod routing;
pub mod system;
pub mod tunnel;
pub mod update;
pub mod vpn;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Instant;

use wardnet_common::config::ApplicationConfiguration;
use wardnetd_data::RepositoryFactory;
use wardnetd_data::repository::DnsRepository;

use crate::auth::AuthServiceImpl;
use crate::backup::BackupServiceImpl;
use crate::backup::archiver::AgeArchiver;
use crate::device::DeviceServiceImpl;
use crate::device::discovery::DeviceDiscoveryServiceImpl;
use crate::dhcp::DhcpServiceImpl;
use crate::dns::DnsServiceImpl;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::jobs::JobServiceImpl;
use crate::routing::RoutingServiceImpl;
use crate::system::SystemServiceImpl;
use crate::tunnel::TunnelServiceImpl;
use crate::update::UpdateServiceImpl;
use crate::vpn::{VpnProviderRegistry, VpnProviderServiceImpl};

pub use crate::auth::AuthService;
pub use crate::backup::BackupService;
pub use crate::device::{DeviceDiscoveryService, DeviceService, ObservationResult};
pub use crate::dhcp::DhcpService;
pub use crate::dns::DnsService;
pub use crate::jobs::{JobService, JobServiceExt, ProgressReporter};
pub use crate::logging::LogService;
pub use crate::routing::RoutingService;
pub use crate::system::SystemService;
pub use crate::tunnel::TunnelService;
pub use crate::update::UpdateService;
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
    pub secret_store: Arc<dyn wardnetd_data::secret_store::SecretStore>,
    pub blocklist_fetcher: Arc<dyn dns::blocklist_downloader::BlocklistFetcher>,
    pub update: UpdateBackends,
    /// Path to the operator-supplied `wardnet.toml` — threaded through
    /// so `BackupService` knows which file to snapshot into a bundle
    /// and which one to overwrite on restore.
    pub config_path: std::path::PathBuf,
    /// Human-readable host identifier stamped into bundle manifests.
    /// Operators see this in the restore preview so they can
    /// double-check they're restoring the right machine.
    pub host_id: String,
    /// Shutdown token shared with `main.rs`'s `shutdown_signal`.
    /// `SystemService::request_restart` cancels this token (rather
    /// than calling `std::process::exit`) so the existing graceful-
    /// shutdown path runs — axum drains connections, runners exit
    /// cleanly, Drop impls fire.
    pub shutdown_token: tokio_util::sync::CancellationToken,
}

/// Auto-update backends, grouped so the three concerns (release discovery,
/// artefact verification, binary swap) travel together but stay individually
/// swappable for unit tests.
pub struct UpdateBackends {
    pub release_source: Arc<dyn update::ReleaseSource>,
    pub verifier: Arc<dyn update::ReleaseVerifier>,
    pub applier: Arc<dyn update::BinaryApplier>,
}

/// All wired services, ready to use.
pub struct Services {
    pub auth: Arc<dyn AuthService>,
    pub backup: Arc<dyn BackupService>,
    pub device: Arc<dyn DeviceService>,
    pub dhcp: Arc<dyn DhcpService>,
    pub dns: Arc<dyn DnsService>,
    pub discovery: Arc<dyn DeviceDiscoveryService>,
    pub log: Arc<dyn LogService>,
    pub vpn_provider: Arc<dyn VpnProviderService>,
    pub routing: Arc<dyn RoutingService>,
    pub system: Arc<dyn SystemService>,
    pub tunnel: Arc<dyn TunnelService>,
    pub update: Arc<dyn UpdateService>,
    pub event_publisher: Arc<dyn EventPublisher>,
    pub jobs: Arc<dyn JobService>,
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
#[allow(clippy::too_many_lines)] // this is the service wiring function — grows with the system
fn create_services(
    repo_factory: &dyn RepositoryFactory,
    backends: Backends,
    config: &ApplicationConfiguration,
    lan_ip: std::net::Ipv4Addr,
    started_at: Instant,
    log_service: Arc<dyn LogService>,
) -> Services {
    // Clone the backup-relevant fields up front. The rest of the
    // backends get moved into their respective services below, so by
    // the time we wire the backup service `backends` is partially
    // moved and we can't refer to it as a whole.
    let backup_secret_store = backends.secret_store.clone();
    let backup_config_path = backends.config_path.clone();
    let backup_host_id = backends.host_id.clone();

    let admin_repo = repo_factory.admin();
    let session_repo = repo_factory.session();
    let api_key_repo = repo_factory.api_key();
    let device_repo = repo_factory.device();
    let system_config_repo = repo_factory.system_config();
    let dhcp_repo = repo_factory.dhcp();
    let dns_repo = repo_factory.dns();
    let tunnel_repo = repo_factory.tunnel();
    let update_repo = repo_factory.update();

    let event_publisher: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(256));
    let job_service: Arc<dyn JobService> = JobServiceImpl::new();

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
        job_service.clone(),
        backends.blocklist_fetcher.clone(),
    ));

    let system_service: Arc<dyn SystemService> = Arc::new(SystemServiceImpl::new(
        system_config_repo,
        tunnel_repo.clone(),
        started_at,
        backends.shutdown_token.clone(),
    ));

    let tunnel_service: Arc<dyn TunnelService> = Arc::new(TunnelServiceImpl::new(
        tunnel_repo.clone(),
        device_repo.clone(),
        backends.tunnel_interface.clone(),
        backends.secret_store.clone(),
        event_publisher.clone(),
    ));

    let registry = Arc::new(VpnProviderRegistry::new(&config.vpn_providers.enabled));
    let vpn_provider_service: Arc<dyn VpnProviderService> = Arc::new(VpnProviderServiceImpl::new(
        registry,
        tunnel_service.clone(),
    ));

    let discovery_service = build_discovery_service(
        device_repo.clone(),
        event_publisher.clone(),
        backends.hostname_resolver,
        lan_ip,
    );

    let routing_service = build_routing_service(
        device_repo,
        tunnel_repo,
        tunnel_service.clone(),
        backends.policy_router,
        backends.firewall,
        config,
    );

    let update_service = build_update_service(
        repo_factory,
        update_repo,
        backends.update,
        event_publisher.clone(),
        config,
    );

    let backup_service = build_backup_service(
        repo_factory,
        backup_secret_store,
        backup_config_path,
        backup_host_id,
        config,
    );

    Services {
        auth: auth_service,
        backup: backup_service,
        device: device_service,
        dhcp: dhcp_service,
        dns: dns_service,
        log: log_service,
        discovery: discovery_service,
        vpn_provider: vpn_provider_service,
        routing: routing_service,
        system: system_service,
        tunnel: tunnel_service,
        update: update_service,
        event_publisher,
        jobs: job_service,
        dns_repo,
    }
}

/// Build the device-discovery service with the LAN subnet derived
/// from `lan_ip` (falls back to a `/24` on invalid inputs).
fn build_discovery_service(
    device_repo: Arc<dyn wardnetd_data::repository::DeviceRepository>,
    event_publisher: Arc<dyn EventPublisher>,
    hostname_resolver: Arc<dyn device::HostnameResolver>,
    lan_ip: std::net::Ipv4Addr,
) -> Arc<dyn DeviceDiscoveryService> {
    let lan_subnet = ipnetwork::Ipv4Network::new(lan_ip, 24).unwrap_or_else(|_| {
        tracing::warn!("failed to create LAN subnet, using /24 default");
        ipnetwork::Ipv4Network::new(lan_ip, 24).expect("valid /24")
    });
    Arc::new(DeviceDiscoveryServiceImpl::new(
        device_repo,
        event_publisher,
        hostname_resolver,
        lan_subnet,
    ))
}

/// Build the routing service from its repository + backend + config
/// dependencies.
fn build_routing_service(
    device_repo: Arc<dyn wardnetd_data::repository::DeviceRepository>,
    tunnel_repo: Arc<dyn wardnetd_data::repository::TunnelRepository>,
    tunnel_service: Arc<dyn TunnelService>,
    policy_router: Arc<dyn routing::PolicyRouter>,
    firewall: Arc<dyn routing::FirewallManager>,
    config: &ApplicationConfiguration,
) -> Arc<dyn RoutingService> {
    Arc::new(RoutingServiceImpl::new(
        device_repo,
        tunnel_repo,
        tunnel_service,
        policy_router,
        firewall,
        config.network.default_policy.clone(),
        config.network.lan_interface.clone(),
    ))
}

/// Build the auto-update service from the factory + update backends
/// + config.
fn build_update_service(
    repo_factory: &dyn RepositoryFactory,
    update_repo: Arc<dyn wardnetd_data::repository::UpdateRepository>,
    update_backends: UpdateBackends,
    event_publisher: Arc<dyn EventPublisher>,
    config: &ApplicationConfiguration,
) -> Arc<dyn UpdateService> {
    Arc::new(UpdateServiceImpl::new(
        system_config_for_update(repo_factory),
        update_repo,
        update_backends.release_source,
        update_backends.verifier,
        update_backends.applier,
        event_publisher,
        config.update.require_signature,
        version::VERSION,
    ))
}

/// Construct the backup service — composes the factory-provided
/// dumper with the shared secret store and reparses the database
/// path from config so `.bak-<ts>` siblings land next to the live DB.
fn build_backup_service(
    repo_factory: &dyn RepositoryFactory,
    secret_store: Arc<dyn wardnetd_data::secret_store::SecretStore>,
    config_path: std::path::PathBuf,
    host_id: String,
    config: &ApplicationConfiguration,
) -> Arc<dyn BackupService> {
    let database_path = std::path::PathBuf::from(&config.database.connection_string);
    Arc::new(BackupServiceImpl::new(
        Arc::new(AgeArchiver::new()),
        repo_factory.dumper(),
        secret_store,
        repo_factory.system_config(),
        database_path,
        config_path,
        version::VERSION,
        host_id,
    ))
}

/// Fresh handle to the `system_config` repo for the update service.
///
/// `system_config_repo` has already been moved into `SystemService`, so we
/// ask the factory for another instance. Each repo trait object wraps an
/// `Arc<SqlitePool>`, so this is cheap.
fn system_config_for_update(
    repo_factory: &dyn RepositoryFactory,
) -> Arc<dyn wardnetd_data::repository::SystemConfigRepository> {
    repo_factory.system_config()
}
