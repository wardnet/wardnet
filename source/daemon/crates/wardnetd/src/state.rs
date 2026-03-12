use std::sync::Arc;
use std::time::Instant;

use crate::config::Config;
use crate::event::EventPublisher;
use crate::service::{
    AuthService, DeviceDiscoveryService, DeviceService, DhcpService, ProviderService,
    RoutingService, SystemService, TunnelService,
};

/// Shared application state, cheaply cloneable via `Arc`.
///
/// Holds service trait objects and configuration. Handlers access services
/// through this struct — the database pool is never exposed directly.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    auth_service: Arc<dyn AuthService>,
    device_service: Arc<dyn DeviceService>,
    dhcp_service: Arc<dyn DhcpService>,
    discovery_service: Arc<dyn DeviceDiscoveryService>,
    provider_service: Arc<dyn ProviderService>,
    routing_service: Arc<dyn RoutingService>,
    system_service: Arc<dyn SystemService>,
    tunnel_service: Arc<dyn TunnelService>,
    event_publisher: Arc<dyn EventPublisher>,
    config: Config,
    started_at: Instant,
}

impl AppState {
    /// Create a new application state with the given services and configuration.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        auth_service: Arc<dyn AuthService>,
        device_service: Arc<dyn DeviceService>,
        dhcp_service: Arc<dyn DhcpService>,
        discovery_service: Arc<dyn DeviceDiscoveryService>,
        provider_service: Arc<dyn ProviderService>,
        routing_service: Arc<dyn RoutingService>,
        system_service: Arc<dyn SystemService>,
        tunnel_service: Arc<dyn TunnelService>,
        event_publisher: Arc<dyn EventPublisher>,
        config: Config,
        started_at: Instant,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                auth_service,
                device_service,
                dhcp_service,
                discovery_service,
                provider_service,
                routing_service,
                system_service,
                tunnel_service,
                event_publisher,
                config,
                started_at,
            }),
        }
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

    #[must_use]
    pub fn discovery_service(&self) -> &dyn DeviceDiscoveryService {
        self.inner.discovery_service.as_ref()
    }

    /// Access the VPN provider service.
    #[must_use]
    pub fn provider_service(&self) -> &dyn ProviderService {
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

    #[must_use]
    pub fn event_publisher(&self) -> &dyn EventPublisher {
        self.inner.event_publisher.as_ref()
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    /// The instant when the daemon started, used to compute uptime.
    #[must_use]
    pub fn started_at(&self) -> Instant {
        self.inner.started_at
    }
}
