use std::sync::Arc;

use crate::config::Config;
use crate::db::DbPools;
use crate::dns_provider::DnsProvider;
use crate::replay_cache::ReplayCache;
use crate::repository::{ChallengeRepository, InstallRepository};

/// Shared application state injected into every Axum handler via
/// [`axum::extract::State`].
///
/// Cloning is cheap — the inner data lives behind an [`Arc`].
#[derive(Clone)]
pub struct AppState(Arc<Inner>);

struct Inner {
    config: Config,
    db: DbPools,
    installs: Arc<dyn InstallRepository>,
    challenges: Arc<dyn ChallengeRepository>,
    dns: Arc<dyn DnsProvider>,
    /// In-memory replay-prevention cache.
    ///
    /// Keyed by `"{install_id}:{timestamp}:{body_hash}"`; prevents a valid
    /// signed request from being replayed within the ±60 s timestamp window.
    replay_cache: Arc<ReplayCache>,
}

impl AppState {
    #[must_use]
    pub fn new(
        config: Config,
        db: DbPools,
        installs: Arc<dyn InstallRepository>,
        challenges: Arc<dyn ChallengeRepository>,
        dns: Arc<dyn DnsProvider>,
    ) -> Self {
        Self(Arc::new(Inner {
            config,
            db,
            installs,
            challenges,
            dns,
            replay_cache: Arc::new(ReplayCache::new()),
        }))
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.0.config
    }

    #[must_use]
    pub(crate) fn db(&self) -> &DbPools {
        &self.0.db
    }

    #[must_use]
    pub fn installs(&self) -> &dyn InstallRepository {
        &*self.0.installs
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
}
