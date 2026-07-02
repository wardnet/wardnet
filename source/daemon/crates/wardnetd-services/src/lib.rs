pub mod auth_context;
pub mod command;
pub mod db_busy;
pub mod db_maintenance_runner;
pub mod error;
pub mod event;
pub mod jobs;
pub mod request_context;
pub mod secret_store;
pub mod stats;
pub mod version;

pub mod auth;
pub mod backup;
pub mod cloud;
pub mod ddns;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod dns_filter;
pub mod dns_local;
pub mod entitlement;
pub mod garp;
pub mod health;
pub mod logging;
pub mod maintenance;
pub mod network_zone;
pub mod push;
pub mod routing;
pub mod rule_request;
pub mod system;
pub mod tls;
pub mod tunnel;
pub mod update;
pub mod vpn;
pub mod zone_enforcement;
pub mod zone_exception;

#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::mpsc;
use wardnet_common::config::ApplicationConfiguration;
use wardnetd_data::RepositoryFactory;
use wardnetd_data::repository::{
    DnsEventsRepository, DnsFilterRepository, DnsRepository, QueryLogRow,
};

use crate::dns::log_sink::{DnsLogSink, DnsLogSinkChannels};
use crate::stats::Meter;
use crate::stats::service::StatsServiceImpl;

use crate::auth::AuthServiceImpl;
use crate::backup::BackupServiceImpl;
use crate::backup::archiver::AgeArchiver;
use crate::ddns::DdnsServiceImpl;
use crate::device::DeviceServiceImpl;
use crate::device::discovery::DeviceDiscoveryServiceImpl;
use crate::dhcp::DhcpServiceImpl;
use crate::dns::DnsServiceImpl;
use crate::dns_filter::DnsFilterServiceImpl;
use crate::dns_local::DnsLocalServiceImpl;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::jobs::JobServiceImpl;
use crate::network_zone::NetworkZoneServiceImpl;
use crate::routing::RoutingServiceImpl;
use crate::rule_request::RuleRequestServiceImpl;
use crate::system::SystemServiceImpl;
use crate::tls::TlsServiceImpl;
use crate::tunnel::TunnelServiceImpl;
use crate::update::UpdateServiceImpl;
use crate::vpn::{VpnProviderRegistry, VpnProviderServiceImpl};
use crate::zone_enforcement::ZoneEnforcementServiceImpl;
use crate::zone_exception::ZoneExceptionServiceImpl;

pub use crate::auth::AuthService;
pub use crate::backup::BackupService;
pub use crate::ddns::DdnsService;
pub use crate::device::{DeviceDiscoveryService, DeviceService, ObservationResult};
pub use crate::dhcp::DhcpService;
pub use crate::dns::DnsService;
pub use crate::dns_filter::DnsFilterService;
pub use crate::dns_local::DnsLocalService;
pub use crate::health::{
    CheckOutcome, ComponentHealth, HealthCheck, HealthMonitor, HealthSnapshot, HealthStatus,
};
pub use crate::jobs::{JobService, JobServiceExt, ProgressReporter};
pub use crate::logging::LogService;
pub use crate::maintenance::{MaintenanceService, MaintenanceServiceImpl};
pub use crate::network_zone::NetworkZoneService;
pub use crate::push::PushService;
pub use crate::routing::RoutingService;
pub use crate::rule_request::RuleRequestService;
pub use crate::stats::{
    DEFAULT_FLUSH_INTERVAL, DEFAULT_MAINTENANCE_INTERVAL, StatsBuffer, StatsFlushRunner,
    StatsService,
};
pub use crate::system::SystemService;
pub use crate::tls::runner::TlsRenewalRunner;
pub use crate::tls::{CertActivator, TlsService, TlsStatus};
pub use crate::tunnel::TunnelService;
pub use crate::update::UpdateService;
pub use crate::vpn::VpnProviderService;
pub use crate::zone_enforcement::ZoneEnforcementService;
pub use crate::zone_exception::ZoneExceptionService;

/// Backends provided by the caller (real or mock).
///
/// The daemon passes real implementations (`WireGuard`, nftables, etc.);
/// the mock server passes no-op stubs.
pub struct Backends {
    pub tunnel_interface: Arc<dyn tunnel::TunnelInterface>,
    /// Sends a single HTTP probe through a tunnel interface to learn the
    /// public IP and country at the tunnel exit. Wired to a real reqwest
    /// implementation in production and to a deterministic mock in
    /// `wardnetd-mock`.
    pub tunnel_exit_probe: Arc<dyn tunnel::TunnelExitProbe>,
    /// Sends a single ICMP echo through a tunnel interface to measure
    /// round-trip latency. Wired to a real surge-ping implementation in
    /// production (Linux-only) and to a deterministic mock in
    /// `wardnetd-mock`.
    pub tunnel_latency_prober: Arc<dyn tunnel::TunnelLatencyProber>,
    /// Downloads a fixed payload to measure throughput, either unbound
    /// (direct/WAN) or bound to a tunnel interface (`SO_BINDTODEVICE`).
    /// Wired to a reqwest-based downloader in production and a fixed-value
    /// stub in the mock.
    pub tunnel_throughput_tester: Arc<dyn tunnel::ThroughputTester>,
    pub policy_router: Arc<dyn routing::PolicyRouter>,
    pub firewall: Arc<dyn routing::FirewallManager>,
    pub packet_capture: Arc<dyn device::PacketCapture>,
    pub hostname_resolver: Arc<dyn device::HostnameResolver>,
    pub secret_store: Arc<dyn wardnetd_data::secret_store::SecretStore>,
    /// Delivers Web Push notifications. Wired to a reqwest-backed
    /// implementation in production and to a no-op recorder in `wardnetd-mock`.
    pub web_push_sender: Arc<dyn push::sender::WebPushSender>,
    pub blocklist_fetcher: Arc<dyn dns_filter::blocklist_downloader::BlocklistFetcher>,
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
    /// Host-power operations. Production wires `SystemctlPowerOps`
    /// (shells out to `systemctl reboot|poweroff --no-block`); the
    /// mock wires a logging no-op so dev launches don't get killed.
    pub power_ops: Arc<dyn system::SystemPowerOps>,
    /// Reads the LAN interface state for the wizard's network step
    /// and the post-install Settings page. Real impl in `wardnetd`
    /// parses `/proc/net/route` and `/etc/dhcpcd.conf.d/wardnet.conf`;
    /// the mock returns synthetic data.
    pub network_inspector: Arc<dyn system::NetworkInspector>,
    /// Active LAN probe — the wizard's router-MAC step uses this to
    /// auto-discover the upstream router via ARP. Real impl wraps
    /// `pnet`'s datalink channel; the mock returns a synthetic MAC.
    pub network_probe: Arc<dyn system::NetworkProbe>,
    /// Gratuitous-ARP failover hook. Real impl in `wardnetd` wraps
    /// `pnet` to send the farewell/claim sequences; the mock is a
    /// logging no-op. See [`garp::GarpOps`] and issue #213.
    pub garp_ops: Arc<dyn garp::GarpOps>,
    /// Hot-swaps the live `:443` certificate. Real impl in `wardnetd` wraps the
    /// `axum-server` `RustlsConfig` + `provisioned` flag; the mock is a no-op.
    /// Injected here (rather than constructed in `create_services`) so the
    /// serving stack — and thus `axum-server` — stays out of this crate. See
    /// [`tls::CertActivator`].
    pub cert_activator: Arc<dyn tls::CertActivator>,
    /// Hardware watchdog operations. Production wires the Linux
    /// `/dev/watchdog` implementation; the mock wires a logging no-op so dev
    /// launches never arm a real device. Pet **ungated** by health — see
    /// [`system::WatchdogOps`] and issue #214.
    pub watchdog_ops: Arc<dyn system::WatchdogOps>,
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
    pub dns_filter: Arc<dyn DnsFilterService>,
    pub dns_local: Arc<dyn DnsLocalService>,
    /// Dynamic-DNS service: registers/keeps the public A record current via the
    /// active provider (bridge or BYOD Cloudflare). Driven by `DdnsUpdateRunner`.
    pub ddns: Arc<dyn DdnsService>,
    /// Daemon-owned TLS: ACME issuance/renewal + live-cert hot-swap. Driven by
    /// `TlsRenewalRunner`; also called by the wizard (C9) and Settings (C10).
    pub tls: Arc<dyn TlsService>,
    pub discovery: Arc<dyn DeviceDiscoveryService>,
    pub log: Arc<dyn LogService>,
    pub vpn_provider: Arc<dyn VpnProviderService>,
    pub routing: Arc<dyn RoutingService>,
    pub rule_request: Arc<dyn RuleRequestService>,
    /// Push notifications: VAPID keys, subscription CRUD, and event-driven Web
    /// Push delivery. Driven by `PushNotificationListener`.
    pub push: Arc<dyn PushService>,
    /// Network Zones: device policy buckets (epic #244, issue #735).
    pub network_zone: Arc<dyn NetworkZoneService>,
    /// Network-Zone packet enforcer: per-zone nftables egress + admin-UI gating
    /// (epic #244, issue #736). Driven by `ZoneEnforcementListener`.
    pub zone_enforcement: Arc<dyn ZoneEnforcementService>,
    /// Cross-zone exceptions: admin-granted per-endpoint allowances across zone
    /// boundaries (epic #244, issue #737).
    pub zone_exception: Arc<dyn ZoneExceptionService>,
    pub system: Arc<dyn SystemService>,
    pub tunnel: Arc<dyn TunnelService>,
    pub update: Arc<dyn UpdateService>,
    pub event_publisher: Arc<dyn EventPublisher>,
    pub jobs: Arc<dyn JobService>,
    pub device_repo: Arc<dyn wardnetd_data::repository::DeviceRepository>,
    pub dns_repo: Arc<dyn DnsRepository>,
    pub dns_filter_repo: Arc<dyn DnsFilterRepository>,
    /// Cross-cutting DB maintenance (incremental vacuum etc.) as an
    /// auth-gated service, so `DbMaintenanceRunner` calls it under an admin
    /// context rather than holding the repository directly.
    pub maintenance: Arc<dyn MaintenanceService>,
    /// Tunnel repository — exposed so the DNS server can resolve
    /// `UpstreamId::Tunnel(_)` entries from the routing snapshot into a
    /// concrete (interface name, DNS upstream) pair without going through
    /// the auth-gated tunnel service. See issue #342.
    pub tunnel_repo: Arc<dyn wardnetd_data::repository::TunnelRepository>,
    /// Shared sink the DNS server writes to and the WS handler subscribes
    /// from. Live for the lifetime of the daemon.
    pub dns_log_sink: Arc<DnsLogSink>,
    /// Persistence receiver, taken by the runner once at startup.
    pub dns_log_persist_rx: Mutex<Option<mpsc::Receiver<QueryLogRow>>>,
    /// Capture receiver, taken by [`DnsCaptureRunner`] once at startup.
    pub dns_capture_rx: Mutex<Option<mpsc::Receiver<QueryLogRow>>>,
    /// DNS events repository — used by the capture runner and API handlers.
    pub dns_events_repo: Arc<dyn DnsEventsRepository>,
    /// Generic pre-aggregating stats service — used by HTTP handlers.
    pub stats: Arc<dyn StatsService>,
    /// Shared stats buffer — drained by [`StatsFlushRunner`] in `main.rs`.
    pub stats_buffer: Arc<StatsBuffer>,
    /// Process-wide entitlement state, flipped by the DDNS cloud clients on
    /// token mints (suspend on a `403`, restore on success). Cloned into
    /// `AppState` (to gate the premium app surfaces) and the DDNS/TLS runners
    /// (to stay inert while suspended). The same handle the `ddns` service holds.
    pub entitlement: Arc<entitlement::Entitlement>,
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

    // Bootstrap setup-wizard + default-policy keys (idempotent — only seeds
    // missing keys, so re-running on upgraded installs is safe).
    let system_config_for_bootstrap = repo_factory.system_config();
    wardnetd_data::bootstrap::bootstrap_system_config(
        &system_config_for_bootstrap,
        &config.network.default_policy,
    )
    .await?;
    let initial_default_policy = system_config_for_bootstrap
        .get_default_policy()
        .await?
        .unwrap_or_else(|| config.network.default_policy.clone());

    // Classify the previous shutdown and arm the running marker for
    // this run. Runs once per boot, before any other component
    // touches `system_config`. Its result is logged here; the API
    // surfaces it on every `/api/system/status` call by reading the
    // persisted classification back from the repository.
    classify_previous_shutdown(repo_factory.as_ref()).await;

    // Wire services.
    Ok(create_services(
        repo_factory.as_ref(),
        backends,
        config,
        lan_ip,
        started_at,
        log_service,
        initial_default_policy,
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
pub async fn init_services_with_factory(
    repo_factory: &dyn RepositoryFactory,
    backends: Backends,
    config: &ApplicationConfiguration,
    lan_ip: std::net::Ipv4Addr,
    started_at: Instant,
    log_service: Arc<dyn LogService>,
) -> anyhow::Result<Services> {
    // Bootstrap setup-wizard + default-policy keys. The mock variant
    // intentionally skips admin bootstrap (so the wizard runs on every
    // launch), but it still benefits from a seeded default_policy and
    // wizard_step.
    let system_config_for_bootstrap = repo_factory.system_config();
    wardnetd_data::bootstrap::bootstrap_system_config(
        &system_config_for_bootstrap,
        &config.network.default_policy,
    )
    .await?;
    let initial_default_policy = system_config_for_bootstrap
        .get_default_policy()
        .await?
        .unwrap_or_else(|| config.network.default_policy.clone());

    // Classify the previous shutdown and arm the running marker for
    // this run. See `init_services` for context.
    classify_previous_shutdown(repo_factory).await;

    Ok(create_services(
        repo_factory,
        backends,
        config,
        lan_ip,
        started_at,
        log_service,
        initial_default_policy,
    ))
}

/// Run [`SystemConfigRepository::detect_previous_shutdown`] once at
/// startup and log the outcome.
///
/// Called from [`init_services`] / [`init_services_with_factory`] so
/// the classification persists into `prev_shutdown_*` keys before any
/// service reads them. Failure here is logged but never fatal — a
/// transient `SQLite` error must not prevent the daemon from booting.
async fn classify_previous_shutdown(repo_factory: &dyn RepositoryFactory) {
    let repo = repo_factory.system_config();
    match repo.detect_previous_shutdown().await {
        Ok(info) => match info.state {
            wardnet_common::api::LastShutdownState::Unclean => {
                tracing::warn!(
                    at = ?info.at,
                    "previous daemon shutdown was unclean (crash or power loss): at={at:?}",
                    at = info.at,
                );
            }
            wardnet_common::api::LastShutdownState::Graceful => {
                tracing::info!(
                    at = ?info.at,
                    "previous daemon shutdown was graceful: at={at:?}",
                    at = info.at,
                );
            }
            wardnet_common::api::LastShutdownState::Unknown => {
                tracing::info!(
                    "no prior shutdown record found (first-ever boot or fresh database)"
                );
            }
        },
        Err(e) => {
            tracing::warn!(
                error = %e,
                "failed to classify previous shutdown — skipping detection: {e}",
            );
        }
    }
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
    initial_default_policy: String,
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
    let network_zone_repo = repo_factory.network_zone();
    let system_config_repo = repo_factory.system_config();
    let dhcp_repo = repo_factory.dhcp();
    let dns_repo = repo_factory.dns();
    let dns_filter_repo = repo_factory.dns_filter();
    let dns_local_repo = repo_factory.dns_local();
    let maintenance_repo = repo_factory.maintenance();
    let tunnel_repo = repo_factory.tunnel();
    let tunnel_speed_test_repo = repo_factory.tunnel_speed_tests();
    let update_repo = repo_factory.update();
    let stats_repo = repo_factory.stats();

    let event_publisher: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(256));
    let job_service: Arc<dyn JobService> = JobServiceImpl::new();

    let stats_buffer = StatsBuffer::new();
    let stats_meter = Arc::new(Meter::new(stats_buffer.clone()));
    let stats_service: Arc<dyn StatsService> = Arc::new(StatsServiceImpl::new(stats_repo));

    let auth_service: Arc<dyn AuthService> = Arc::new(AuthServiceImpl::new(
        admin_repo,
        session_repo,
        api_key_repo,
        system_config_repo.clone(),
        config.auth.session_expiry_hours,
        config.auth.remember_me_expiry_hours,
    ));

    let dns_events_repo = repo_factory.dns_events();

    let device_service: Arc<dyn DeviceService> = Arc::new(DeviceServiceImpl::new(
        device_repo.clone(),
        dns_events_repo.clone(),
        network_zone_repo.clone(),
        system_config_repo.clone(),
        event_publisher.clone(),
    ));

    let network_zone_service: Arc<dyn NetworkZoneService> = Arc::new(NetworkZoneServiceImpl::new(
        network_zone_repo.clone(),
        device_repo.clone(),
        system_config_repo.clone(),
        event_publisher.clone(),
    ));

    let zone_exception_service: Arc<dyn ZoneExceptionService> =
        Arc::new(ZoneExceptionServiceImpl::new(
            repo_factory.zone_exception(),
            network_zone_repo.clone(),
            device_repo.clone(),
            event_publisher.clone(),
        ));

    let rule_request_service: Arc<dyn RuleRequestService> = Arc::new(RuleRequestServiceImpl::new(
        repo_factory.rule_request(),
        device_service.clone(),
    ));

    let push_service: Arc<dyn PushService> = Arc::new(crate::push::PushServiceImpl::new(
        repo_factory.push(),
        device_repo.clone(),
        tunnel_repo.clone(),
        system_config_repo.clone(),
        backends.secret_store.clone(),
        backends.web_push_sender.clone(),
    ));

    let dhcp_service: Arc<dyn DhcpService> = Arc::new(DhcpServiceImpl::new(
        dhcp_repo.clone(),
        system_config_repo.clone(),
        event_publisher.clone(),
        lan_ip,
    ));

    let (
        dns_log_sink,
        DnsLogSinkChannels {
            persist_rx: dns_log_persist_rx,
            capture_rx: dns_capture_rx,
        },
    ) = DnsLogSink::new_with_stats(&stats_meter);
    let dns_service: Arc<dyn DnsService> = Arc::new(DnsServiceImpl::new(
        system_config_repo.clone(),
        dns_repo.clone(),
        event_publisher.clone(),
        Some(dns_log_sink.clone()),
    ));

    let dns_filter_service: Arc<dyn DnsFilterService> = Arc::new(DnsFilterServiceImpl::new(
        dns_filter_repo.clone(),
        device_repo.clone(),
        event_publisher.clone(),
        job_service.clone(),
        backends.blocklist_fetcher.clone(),
    ));

    let dns_local_service: Arc<dyn DnsLocalService> = Arc::new(DnsLocalServiceImpl::new(
        dns_local_repo.clone(),
        event_publisher.clone(),
    ));

    // DDNS service — reads/writes provider config in `system_config` (fresh
    // handle) and credentials in the shared secret store. Constructs the active
    // provider internally from stored config; no extra `Backends` field needed.
    //
    // Built concretely so we can lift the shared `Entitlement` handle out before
    // boxing it as a trait object: the cloud clients inside the service flip it
    // on token mints, and the composition root clones it into `AppState` and the
    // background runners so the whole daemon reads one suspended state.
    let ddns_impl =
        DdnsServiceImpl::new(repo_factory.system_config(), backends.secret_store.clone());
    let entitlement = ddns_impl.entitlement();
    let ddns: Arc<dyn DdnsService> = Arc::new(ddns_impl);

    // TLS service — ACME issuance/renewal. Publishes DNS-01 challenges through
    // `ddns`, persists cert material in the shared secret store, and hot-swaps
    // the live `:443` cert via the `wardnetd`-provided `CertActivator`. Idle
    // until DDNS is configured (no active FQDN → issuance is inert).
    let tls: Arc<dyn TlsService> = Arc::new(TlsServiceImpl::new(
        repo_factory.system_config(),
        backends.secret_store.clone(),
        ddns.clone(),
        backends.cert_activator.clone(),
        dns_local_service.clone(),
        lan_ip,
    ));

    let maintenance_service: Arc<dyn MaintenanceService> =
        Arc::new(MaintenanceServiceImpl::new(maintenance_repo));

    let system_service: Arc<dyn SystemService> = Arc::new(SystemServiceImpl::new(
        system_config_repo,
        tunnel_repo.clone(),
        started_at,
        backends.shutdown_token.clone(),
        backends.power_ops.clone(),
        backends.network_inspector.clone(),
        backends.network_probe.clone(),
        config.database.to_file_path(),
    ));

    let registry = Arc::new(VpnProviderRegistry::new(&config.vpn_providers.enabled));

    let tunnel_service: Arc<dyn TunnelService> = Arc::new(TunnelServiceImpl::new(
        tunnel_repo.clone(),
        device_repo.clone(),
        backends.tunnel_interface.clone(),
        backends.tunnel_exit_probe.clone(),
        backends.tunnel_latency_prober.clone(),
        backends.tunnel_throughput_tester.clone(),
        backends.secret_store.clone(),
        event_publisher.clone(),
        stats_meter.clone(),
        registry.clone(),
        job_service.clone(),
        tunnel_speed_test_repo.clone(),
        config.tunnel.speed_test_latency_samples,
    ));

    let vpn_provider_service: Arc<dyn VpnProviderService> = Arc::new(VpnProviderServiceImpl::new(
        registry,
        tunnel_service.clone(),
    ));

    let discovery_service = build_discovery_service(
        device_repo.clone(),
        network_zone_repo.clone(),
        dhcp_repo,
        event_publisher.clone(),
        backends.hostname_resolver,
        lan_ip,
    );

    let routing_service = build_routing_service(
        device_repo.clone(),
        tunnel_repo.clone(),
        tunnel_service.clone(),
        backends.policy_router.clone(),
        backends.firewall.clone(),
        repo_factory.system_config(),
        event_publisher.clone(),
        initial_default_policy,
        config,
    );

    // The Network-Zone enforcer (#736) shares the same firewall + policy-router
    // backends as the routing service (they cooperate on the `wardnet` nftables
    // table and per-device conntrack) and calls back into the routing service to
    // unbind tunnel bindings a device's zone forbids after a default-policy flip.
    let zone_enforcement_service = build_zone_enforcement_service(
        network_zone_repo.clone(),
        device_repo.clone(),
        repo_factory.system_config(),
        backends.firewall,
        backends.policy_router,
        routing_service.clone(),
        config,
    );

    let update_service = build_update_service(
        repo_factory,
        update_repo,
        backends.update,
        event_publisher.clone(),
        config,
        backends.shutdown_token.clone(),
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
        dns_filter: dns_filter_service,
        dns_local: dns_local_service,
        ddns,
        tls,
        log: log_service,
        discovery: discovery_service,
        vpn_provider: vpn_provider_service,
        routing: routing_service,
        network_zone: network_zone_service,
        zone_enforcement: zone_enforcement_service,
        zone_exception: zone_exception_service,
        rule_request: rule_request_service,
        push: push_service,
        system: system_service,
        tunnel: tunnel_service,
        update: update_service,
        event_publisher,
        jobs: job_service,
        device_repo,
        dns_repo,
        dns_filter_repo,
        maintenance: maintenance_service,
        tunnel_repo,
        dns_log_sink,
        dns_log_persist_rx: Mutex::new(Some(dns_log_persist_rx)),
        dns_capture_rx: Mutex::new(Some(dns_capture_rx)),
        dns_events_repo,
        stats: stats_service,
        stats_buffer,
        entitlement,
    }
}

/// Build the device-discovery service with the LAN subnet derived
/// from `lan_ip` (falls back to a `/24` on invalid inputs).
fn build_discovery_service(
    device_repo: Arc<dyn wardnetd_data::repository::DeviceRepository>,
    network_zone_repo: Arc<dyn wardnetd_data::repository::NetworkZoneRepository>,
    dhcp_repo: Arc<dyn wardnetd_data::repository::DhcpRepository>,
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
        network_zone_repo,
        dhcp_repo,
        event_publisher,
        hostname_resolver,
        lan_subnet,
    ))
}

/// Build the routing service from its repository + backend + config
/// dependencies.
#[allow(clippy::too_many_arguments)]
fn build_routing_service(
    device_repo: Arc<dyn wardnetd_data::repository::DeviceRepository>,
    tunnel_repo: Arc<dyn wardnetd_data::repository::TunnelRepository>,
    tunnel_service: Arc<dyn TunnelService>,
    policy_router: Arc<dyn routing::PolicyRouter>,
    firewall: Arc<dyn routing::FirewallManager>,
    system_config: Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
    events: Arc<dyn EventPublisher>,
    initial_default_policy: String,
    config: &ApplicationConfiguration,
) -> Arc<dyn RoutingService> {
    Arc::new(RoutingServiceImpl::new(
        device_repo,
        tunnel_repo,
        tunnel_service,
        policy_router,
        firewall,
        system_config,
        events,
        initial_default_policy,
        config.network.lan_interface.clone(),
    ))
}

/// Build the Network-Zone enforcer (#736) from its repository + backend
/// dependencies. It shares the firewall + policy-router backends with the
/// routing service and holds a handle to the routing service so it can unbind
/// tunnel bindings a device's zone forbids after a default-policy flip.
#[allow(clippy::too_many_arguments)]
fn build_zone_enforcement_service(
    network_zone_repo: Arc<dyn wardnetd_data::repository::NetworkZoneRepository>,
    device_repo: Arc<dyn wardnetd_data::repository::DeviceRepository>,
    system_config: Arc<dyn wardnetd_data::repository::SystemConfigRepository>,
    firewall: Arc<dyn routing::FirewallManager>,
    policy_router: Arc<dyn routing::PolicyRouter>,
    routing_service: Arc<dyn RoutingService>,
    config: &ApplicationConfiguration,
) -> Arc<dyn ZoneEnforcementService> {
    Arc::new(ZoneEnforcementServiceImpl::new(
        network_zone_repo,
        device_repo,
        system_config,
        firewall,
        policy_router,
        routing_service,
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
    shutdown_token: tokio_util::sync::CancellationToken,
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
        shutdown_token,
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
    let database_path = config.database.to_file_path().unwrap_or_default();
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
