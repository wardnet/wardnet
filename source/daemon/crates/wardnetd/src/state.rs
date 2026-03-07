use std::sync::Arc;

use crate::config::Config;
use crate::service::{AuthService, DeviceService, SystemService};

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
    system_service: Arc<dyn SystemService>,
    config: Config,
}

impl AppState {
    pub fn new(
        auth_service: Arc<dyn AuthService>,
        device_service: Arc<dyn DeviceService>,
        system_service: Arc<dyn SystemService>,
        config: Config,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                auth_service,
                device_service,
                system_service,
                config,
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

    #[must_use]
    pub fn system_service(&self) -> &dyn SystemService {
        self.inner.system_service.as_ref()
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.inner.config
    }
}
