use std::sync::Arc;

use crate::config::Config;
use crate::db::DbPools;
use crate::dns_provider::DnsProvider;
use crate::replay_cache::ReplayCache;
use crate::repository::{ChallengeRepository, InstallRepository, NameRepository};
use crate::tunnel::TunnelRegistry;

/// Shared application state injected into every Axum handler via
/// [`axum::extract::State`].
///
/// Cloning is cheap — the inner data lives behind an [`Arc`].
#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    config: Config,
    installs: Arc<dyn InstallRepository>,
    /// Global naming authority — vanity-slug allocation across the fleet.
    names: Arc<dyn NameRepository>,
    challenges: Arc<dyn ChallengeRepository>,
    dns: Arc<dyn DnsProvider>,
    /// In-memory replay-prevention cache.
    ///
    /// Keyed by `"{install_id}:{timestamp}:{body_hash}"`; prevents a valid
    /// signed request from being replayed within the ±60 s timestamp window.
    replay_cache: Arc<ReplayCache>,
    /// Registry of active Pi reverse-tunnel WebSocket connections.
    tunnel_registry: Arc<TunnelRegistry>,
}

impl AppState {
    #[must_use]
    pub fn new(
        config: Config,
        _db: DbPools,
        installs: Arc<dyn InstallRepository>,
        names: Arc<dyn NameRepository>,
        challenges: Arc<dyn ChallengeRepository>,
        dns: Arc<dyn DnsProvider>,
        tunnel_registry: Arc<TunnelRegistry>,
    ) -> Self {
        Self(Arc::new(Inner {
            config,
            installs,
            names,
            challenges,
            dns,
            replay_cache: Arc::new(ReplayCache::new()),
            tunnel_registry,
        }))
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.0.config
    }

    #[must_use]
    pub fn installs(&self) -> &dyn InstallRepository {
        &*self.0.installs
    }

    #[must_use]
    pub fn names(&self) -> &dyn NameRepository {
        &*self.0.names
    }

    #[must_use]
    pub fn challenges(&self) -> &dyn ChallengeRepository {
        &*self.0.challenges
    }

    #[must_use]
    pub fn dns(&self) -> &dyn DnsProvider {
        &*self.0.dns
    }

    #[must_use]
    pub(crate) fn replay_cache(&self) -> &ReplayCache {
        &self.0.replay_cache
    }

    /// Returns a cloned `Arc` to the tunnel registry.
    #[must_use]
    pub fn tunnel_registry(&self) -> Arc<TunnelRegistry> {
        Arc::clone(&self.0.tunnel_registry)
    }
}
