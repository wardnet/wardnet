use std::sync::Arc;

use wardnetd_services::dhcp::server::DhcpServer;
use wardnetd_services::dns::server::DnsServer;
use wardnetd_services::event::EventPublisher;
use wardnetd_services::{
    AuthService, BackupService, DeviceDiscoveryService, DeviceService, DhcpService,
    DnsFilterService, DnsService, JobService, LogService, RoutingService, StatsService,
    SystemService, TunnelService, UpdateService, VpnProviderService,
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
}

impl AppState {
    /// Create a new application state with the given services.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        auth_service: Arc<dyn AuthService>,
        backup_service: Arc<dyn BackupService>,
        device_service: Arc<dyn DeviceService>,
        dhcp_service: Arc<dyn DhcpService>,
        dns_service: Arc<dyn DnsService>,
        dns_filter_service: Arc<dyn DnsFilterService>,
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
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                auth_service,
                backup_service,
                device_service,
                dhcp_service,
                dns_service,
                dns_filter_service,
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
            }),
        }
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
}
