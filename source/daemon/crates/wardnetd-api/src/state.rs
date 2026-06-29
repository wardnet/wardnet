use std::sync::Arc;

use wardnetd_services::dhcp::server::DhcpServer;
use wardnetd_services::dns::server::DnsServer;
use wardnetd_services::entitlement::Entitlement;
use wardnetd_services::event::EventPublisher;
use wardnetd_services::{
    AuthService, BackupService, DdnsService, DeviceDiscoveryService, DeviceService, DhcpService,
    DnsFilterService, DnsLocalService, DnsService, HealthMonitor, JobService, LogService,
    RoutingService, RuleRequestService, StatsService, SystemService, TlsService, TunnelService,
    UpdateService, VpnProviderService,
};

/// Shared application state, cheaply cloneable via `Arc`.
///
/// Holds service trait objects. Handlers access services through this struct —
/// the database pool is never exposed directly.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    auth_service: Arc<dyn AuthService>,
    backup_service: Arc<dyn BackupService>,
    device_service: Arc<dyn DeviceService>,
    dhcp_service: Arc<dyn DhcpService>,
    dns_service: Arc<dyn DnsService>,
    dns_filter_service: Arc<dyn DnsFilterService>,
    dns_local_service: Arc<dyn DnsLocalService>,
    ddns_service: Arc<dyn DdnsService>,
    tls_service: Arc<dyn TlsService>,
    discovery_service: Arc<dyn DeviceDiscoveryService>,
    log_service: Arc<dyn LogService>,
    provider_service: Arc<dyn VpnProviderService>,
    routing_service: Arc<dyn RoutingService>,
    system_service: Arc<dyn SystemService>,
    tunnel_service: Arc<dyn TunnelService>,
    update_service: Arc<dyn UpdateService>,
    dhcp_server: Arc<dyn DhcpServer>,
    dns_server: Arc<dyn DnsServer>,
    event_publisher: Arc<dyn EventPublisher>,
    job_service: Arc<dyn JobService>,
    stats_service: Arc<dyn StatsService>,
    rule_request_service: Arc<dyn RuleRequestService>,
    health_monitor: Arc<HealthMonitor>,
    /// Process-wide entitlement state. Read by the serving layer to gate the
    /// premium app surfaces (user PWA + admin mobile app) while suspended, and
    /// surfaced to handlers. Defaults to an active handle in [`Self::new`];
    /// production and the mock inject the live one (the same handle the DDNS
    /// cloud clients flip) via [`Self::with_entitlement`].
    entitlement: Arc<Entitlement>,
}

impl AppState {
    /// Create a new application state with the given services.
    // `ddns_service` vs `dns_service` trips the similar-names lint, but both are
    // established domain terms (local DNS vs dynamic DNS) — renaming either to
    // satisfy the lint would be less clear, not more.
    #[allow(clippy::too_many_arguments, clippy::similar_names)]
    pub fn new(
        auth_service: Arc<dyn AuthService>,
        backup_service: Arc<dyn BackupService>,
        device_service: Arc<dyn DeviceService>,
        dhcp_service: Arc<dyn DhcpService>,
        dns_service: Arc<dyn DnsService>,
        dns_filter_service: Arc<dyn DnsFilterService>,
        dns_local_service: Arc<dyn DnsLocalService>,
        ddns_service: Arc<dyn DdnsService>,
        tls_service: Arc<dyn TlsService>,
        discovery_service: Arc<dyn DeviceDiscoveryService>,
        log_service: Arc<dyn LogService>,
        provider_service: Arc<dyn VpnProviderService>,
        routing_service: Arc<dyn RoutingService>,
        system_service: Arc<dyn SystemService>,
        tunnel_service: Arc<dyn TunnelService>,
        update_service: Arc<dyn UpdateService>,
        dhcp_server: Arc<dyn DhcpServer>,
        dns_server: Arc<dyn DnsServer>,
        event_publisher: Arc<dyn EventPublisher>,
        job_service: Arc<dyn JobService>,
        stats_service: Arc<dyn StatsService>,
        rule_request_service: Arc<dyn RuleRequestService>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                auth_service,
                backup_service,
                device_service,
                dhcp_service,
                dns_service,
                dns_filter_service,
                dns_local_service,
                ddns_service,
                tls_service,
                discovery_service,
                log_service,
                provider_service,
                routing_service,
                system_service,
                tunnel_service,
                update_service,
                dhcp_server,
                dns_server,
                event_publisher,
                job_service,
                stats_service,
                rule_request_service,
                // Defaults to an empty monitor (initial snapshot is UP with no
                // components). Production and the mock inject the live monitor
                // — wired to the runners — via `with_health_monitor`.
                health_monitor: Arc::new(HealthMonitor::new(1, std::time::Duration::from_secs(1))),
                // Defaults to active (never suspended). Production and the mock
                // inject the live handle via `with_entitlement`.
                entitlement: Entitlement::shared(),
            }),
        }
    }

    /// Inject the live [`Entitlement`] handle the DDNS cloud clients flip on
    /// token mints. Returns `self` for chaining off [`Self::new`].
    ///
    /// Like [`Self::with_health_monitor`], this must be called before the state
    /// is cloned or shared — it mutates the not-yet-shared `Arc<Inner>` in place.
    #[must_use]
    pub fn with_entitlement(mut self, entitlement: Arc<Entitlement>) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_entitlement must be called before AppState is cloned")
            .entitlement = entitlement;
        self
    }

    /// Inject the live [`HealthMonitor`] that the health/watchdog runners
    /// drive (issue #214). Returns `self` for chaining off [`Self::new`].
    ///
    /// Must be called before the state is cloned or shared — it mutates the
    /// not-yet-shared `Arc<Inner>` in place. Panics otherwise (a wiring bug).
    #[must_use]
    pub fn with_health_monitor(mut self, health_monitor: Arc<HealthMonitor>) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_health_monitor must be called before AppState is cloned")
            .health_monitor = health_monitor;
        self
    }

    /// Access the backup service.
    #[must_use]
    pub fn backup_service(&self) -> &dyn BackupService {
        self.inner.backup_service.as_ref()
    }

    #[must_use]
    pub fn auth_service(&self) -> &dyn AuthService {
        self.inner.auth_service.as_ref()
    }

    #[must_use]
    pub fn device_service(&self) -> &dyn DeviceService {
        self.inner.device_service.as_ref()
    }

    /// Access the DHCP service.
    #[must_use]
    pub fn dhcp_service(&self) -> &dyn DhcpService {
        self.inner.dhcp_service.as_ref()
    }

    /// Access the DNS service.
    #[must_use]
    pub fn dns_service(&self) -> &dyn DnsService {
        self.inner.dns_service.as_ref()
    }

    /// Access the DNS filter service (profiles, blocklists, allowlist,
    /// custom rules, per-device settings — issue #221).
    #[must_use]
    pub fn dns_filter_service(&self) -> &dyn DnsFilterService {
        self.inner.dns_filter_service.as_ref()
    }

    /// Access the local-DNS service (authoritative zones, custom records,
    /// forwarding rules — issue #217).
    #[must_use]
    pub fn dns_local_service(&self) -> &dyn DnsLocalService {
        self.inner.dns_local_service.as_ref()
    }

    /// Access the dynamic-DNS service (bridge + BYOD-Cloudflare registration,
    /// public-IP publishing — issues #527/#530).
    #[must_use]
    pub fn ddns_service(&self) -> &dyn DdnsService {
        self.inner.ddns_service.as_ref()
    }

    /// Clone the `Arc` for the DDNS service, for moving into a background task.
    #[must_use]
    pub fn ddns_service_arc(&self) -> Arc<dyn DdnsService> {
        self.inner.ddns_service.clone()
    }

    /// Access the daemon-owned TLS service (ACME issuance/renewal, provisioning
    /// status — issues #528/#530).
    #[must_use]
    pub fn tls_service(&self) -> &dyn TlsService {
        self.inner.tls_service.as_ref()
    }

    /// Clone the `Arc` for the TLS service, for moving into a background task.
    #[must_use]
    pub fn tls_service_arc(&self) -> Arc<dyn TlsService> {
        self.inner.tls_service.clone()
    }

    #[must_use]
    pub fn discovery_service(&self) -> &dyn DeviceDiscoveryService {
        self.inner.discovery_service.as_ref()
    }

    /// Access the log service (streaming, errors, file download).
    #[must_use]
    pub fn log_service(&self) -> &dyn LogService {
        self.inner.log_service.as_ref()
    }

    /// Access the VPN provider service.
    #[must_use]
    pub fn provider_service(&self) -> &dyn VpnProviderService {
        self.inner.provider_service.as_ref()
    }

    /// Access the policy routing service.
    #[must_use]
    pub fn routing_service(&self) -> &dyn RoutingService {
        self.inner.routing_service.as_ref()
    }

    #[must_use]
    pub fn system_service(&self) -> &dyn SystemService {
        self.inner.system_service.as_ref()
    }

    #[must_use]
    pub fn tunnel_service(&self) -> &dyn TunnelService {
        self.inner.tunnel_service.as_ref()
    }

    /// Owned handle to the tunnel service, for methods whose receiver is
    /// `Arc<Self>` (e.g. [`TunnelService::start_speed_test`], which moves a
    /// clone into a background job).
    #[must_use]
    pub fn tunnel_service_arc(&self) -> Arc<dyn TunnelService> {
        self.inner.tunnel_service.clone()
    }

    /// Access the auto-update service.
    #[must_use]
    pub fn update_service(&self) -> &dyn UpdateService {
        self.inner.update_service.as_ref()
    }

    #[must_use]
    pub fn event_publisher(&self) -> &dyn EventPublisher {
        self.inner.event_publisher.as_ref()
    }

    /// Access the DHCP server for start/stop control.
    #[must_use]
    pub fn dhcp_server(&self) -> &dyn DhcpServer {
        self.inner.dhcp_server.as_ref()
    }

    /// Access the DNS server for start/stop/cache control.
    #[must_use]
    pub fn dns_server(&self) -> &dyn DnsServer {
        self.inner.dns_server.as_ref()
    }

    /// Access the background-job executor used by handlers that dispatch
    /// async work and by the `/api/jobs/:id` polling endpoint.
    #[must_use]
    pub fn job_service(&self) -> &dyn JobService {
        self.inner.job_service.as_ref()
    }

    /// Access the generic stats service for time-series and top-N queries.
    #[must_use]
    pub fn stats_service(&self) -> &dyn StatsService {
        self.inner.stats_service.as_ref()
    }

    /// Access the rule-request inbox service.
    #[must_use]
    pub fn rule_request_service(&self) -> &dyn RuleRequestService {
        self.inner.rule_request_service.as_ref()
    }

    /// Access the health monitor for the unauthenticated `GET /health`
    /// endpoint (issue #214).
    #[must_use]
    pub fn health_monitor(&self) -> &HealthMonitor {
        self.inner.health_monitor.as_ref()
    }

    /// Whether the wardnet subscription is currently suspended. The serving
    /// layer reads this to gate the premium app surfaces (user PWA `/` + admin
    /// mobile app `/admin-app/`) while leaving the admin website `/admin/` and
    /// `/api/*` reachable so the operator can always resubscribe.
    #[must_use]
    pub fn is_suspended(&self) -> bool {
        self.inner.entitlement.is_suspended()
    }

    /// Clone the shared [`Entitlement`] handle, for moving into the serving
    /// layer or a background task.
    #[must_use]
    pub fn entitlement(&self) -> Arc<Entitlement> {
        self.inner.entitlement.clone()
    }
}
