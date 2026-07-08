pub mod bootstrap;
pub mod database_dumper;
pub mod db;
pub mod oui;
pub mod repository;
pub mod secret_store;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use wardnet_common::config::{ApplicationConfiguration, DatabaseProvider};

use crate::db::DbPools;
use repository::{
    AdminRepository, ApiKeyRepository, DeviceRepository, DhcpRepository, DnsEventsRepository,
    DnsFilterRepository, DnsLocalRepository, DnsRepository, InboundWgPeerRepository,
    MaintenanceRepository, NetworkZoneRepository, NotificationRepository, PushRepository,
    RuleRequestRepository, SessionRepository, SqliteAdminRepository, SqliteApiKeyRepository,
    SqliteDeviceRepository, SqliteDhcpRepository, SqliteDnsEventsRepository,
    SqliteDnsFilterRepository, SqliteDnsLocalRepository, SqliteDnsRepository,
    SqliteInboundWgPeerRepository, SqliteMaintenanceRepository, SqliteNetworkZoneRepository,
    SqliteNotificationRepository, SqlitePushRepository, SqliteRuleRequestRepository,
    SqliteSessionRepository, SqliteStatsRepository, SqliteSystemConfigRepository,
    SqliteTunnelRepository, SqliteTunnelSpeedTestRepository, SqliteUpdateRepository,
    SqliteZoneExceptionRepository, StatsRepository, SystemConfigRepository, TunnelRepository,
    TunnelSpeedTestRepository, UpdateRepository, ZoneExceptionRepository,
};
use sqlx::SqlitePool;

/// Abstracts repository creation from the underlying database engine.
///
/// `SQLite` today, `PostgreSQL` or `rqlite` for future HA.
pub trait RepositoryFactory: Send + Sync {
    fn admin(&self) -> Arc<dyn AdminRepository>;
    fn session(&self) -> Arc<dyn SessionRepository>;
    fn api_key(&self) -> Arc<dyn ApiKeyRepository>;
    fn device(&self) -> Arc<dyn DeviceRepository>;
    fn network_zone(&self) -> Arc<dyn NetworkZoneRepository>;
    fn zone_exception(&self) -> Arc<dyn ZoneExceptionRepository>;
    fn system_config(&self) -> Arc<dyn SystemConfigRepository>;
    fn dhcp(&self) -> Arc<dyn DhcpRepository>;
    fn dns(&self) -> Arc<dyn DnsRepository>;
    fn dns_events(&self) -> Arc<dyn DnsEventsRepository>;
    fn dns_filter(&self) -> Arc<dyn DnsFilterRepository>;
    fn dns_local(&self) -> Arc<dyn DnsLocalRepository>;
    fn tunnel(&self) -> Arc<dyn TunnelRepository>;
    fn tunnel_speed_tests(&self) -> Arc<dyn TunnelSpeedTestRepository>;
    fn inbound_wg_peers(&self) -> Arc<dyn InboundWgPeerRepository>;
    fn stats(&self) -> Arc<dyn StatsRepository>;
    fn update(&self) -> Arc<dyn UpdateRepository>;
    fn maintenance(&self) -> Arc<dyn MaintenanceRepository>;
    fn rule_request(&self) -> Arc<dyn RuleRequestRepository>;
    fn push(&self) -> Arc<dyn PushRepository>;
    fn notification(&self) -> Arc<dyn NotificationRepository>;

    /// Provider-specific database dumper for backup/restore.
    ///
    /// Kept on the factory rather than on a service wiring seam so the
    /// dumper implementation and the repository implementations stay
    /// in one place per backend — a future non-`SQLite` provider
    /// ships its own dumper alongside its own repositories without
    /// touching the backup service layer.
    fn dumper(&self) -> Arc<dyn database_dumper::DatabaseDumper>;
}

/// Create a [`RepositoryFactory`] from the application configuration.
///
/// Reads `database.provider` and `database.connection_string`, initializes
/// the connection pool, and returns the appropriate factory. Currently only
/// `SQLite` is supported — any other provider returns an error.
pub async fn create_repository_factory(
    config: &ApplicationConfiguration,
) -> anyhow::Result<Box<dyn RepositoryFactory>> {
    match config.database.provider {
        DatabaseProvider::Sqlite => {
            let factory =
                SqliteRepositoryFactory::connect(&config.database.connection_string).await?;
            Ok(Box::new(factory))
        }
    }
}

/// `SQLite`-backed repository factory.
///
/// Holds a [`DbPools`] pair (single-connection writer + multi-connection
/// reader). Every repository is handed the *same* `DbPools` and decides
/// per-method which pool to use; see the individual `Sqlite*Repository`
/// implementations.
pub struct SqliteRepositoryFactory {
    pools: DbPools,
    database_path: std::path::PathBuf,
}

impl SqliteRepositoryFactory {
    /// Initialise a new factory: open the connection pools against
    /// `connection_string`, run migrations, and bind everything to the
    /// factory instance. The production entry point called from
    /// [`create_repository_factory`].
    pub async fn connect(connection_string: &str) -> anyhow::Result<Self> {
        let pools = db::init_db_pools_from_connection_string(connection_string).await?;
        let database_path = std::path::PathBuf::from(connection_string);
        Ok(Self {
            pools,
            database_path,
        })
    }

    /// Construct from an already-initialised single pool.
    ///
    /// Used by the mock (which pre-seeds data into an in-memory pool
    /// and needs to hand the populated pool to the service layer) and
    /// by tests. The reader and writer collapse to the same pool —
    /// fine for in-memory and integration testing, where there's no
    /// real lock contention. Production wiring should go through
    /// [`Self::connect`], which keeps pool creation inside the
    /// factory's lifecycle and splits read from write.
    #[must_use]
    pub fn from_pool(pool: SqlitePool, database_path: std::path::PathBuf) -> Self {
        Self {
            pools: DbPools::single(pool),
            database_path,
        }
    }

    /// Construct from a pre-built [`DbPools`]. Mirrors [`Self::from_pool`]
    /// for callers that already split their pools.
    #[must_use]
    pub fn from_pools(pools: DbPools, database_path: std::path::PathBuf) -> Self {
        Self {
            pools,
            database_path,
        }
    }
}

impl RepositoryFactory for SqliteRepositoryFactory {
    fn admin(&self) -> Arc<dyn AdminRepository> {
        Arc::new(SqliteAdminRepository::new_pools(self.pools.clone()))
    }

    fn session(&self) -> Arc<dyn SessionRepository> {
        Arc::new(SqliteSessionRepository::new_pools(self.pools.clone()))
    }

    fn api_key(&self) -> Arc<dyn ApiKeyRepository> {
        Arc::new(SqliteApiKeyRepository::new_pools(self.pools.clone()))
    }

    fn device(&self) -> Arc<dyn DeviceRepository> {
        Arc::new(SqliteDeviceRepository::new_pools(self.pools.clone()))
    }

    fn network_zone(&self) -> Arc<dyn NetworkZoneRepository> {
        Arc::new(SqliteNetworkZoneRepository::new_pools(self.pools.clone()))
    }

    fn zone_exception(&self) -> Arc<dyn ZoneExceptionRepository> {
        Arc::new(SqliteZoneExceptionRepository::new_pools(self.pools.clone()))
    }

    fn system_config(&self) -> Arc<dyn SystemConfigRepository> {
        Arc::new(SqliteSystemConfigRepository::new_pools(self.pools.clone()))
    }

    fn dhcp(&self) -> Arc<dyn DhcpRepository> {
        Arc::new(SqliteDhcpRepository::new_pools(self.pools.clone()))
    }

    fn dns(&self) -> Arc<dyn DnsRepository> {
        Arc::new(SqliteDnsRepository::new_pools(self.pools.clone()))
    }

    fn dns_events(&self) -> Arc<dyn DnsEventsRepository> {
        Arc::new(SqliteDnsEventsRepository::new_pools(self.pools.clone()))
    }

    fn dns_filter(&self) -> Arc<dyn DnsFilterRepository> {
        Arc::new(SqliteDnsFilterRepository::new_pools(self.pools.clone()))
    }

    fn dns_local(&self) -> Arc<dyn DnsLocalRepository> {
        Arc::new(SqliteDnsLocalRepository::new_pools(self.pools.clone()))
    }

    fn tunnel(&self) -> Arc<dyn TunnelRepository> {
        Arc::new(SqliteTunnelRepository::new_pools(self.pools.clone()))
    }

    fn tunnel_speed_tests(&self) -> Arc<dyn TunnelSpeedTestRepository> {
        Arc::new(SqliteTunnelSpeedTestRepository::new_pools(
            self.pools.clone(),
        ))
    }

    fn inbound_wg_peers(&self) -> Arc<dyn InboundWgPeerRepository> {
        Arc::new(SqliteInboundWgPeerRepository::new_pools(self.pools.clone()))
    }

    fn stats(&self) -> Arc<dyn StatsRepository> {
        Arc::new(SqliteStatsRepository::new_pools(self.pools.clone()))
    }

    fn update(&self) -> Arc<dyn UpdateRepository> {
        Arc::new(SqliteUpdateRepository::new_pools(self.pools.clone()))
    }

    fn maintenance(&self) -> Arc<dyn MaintenanceRepository> {
        Arc::new(SqliteMaintenanceRepository::new_pools(self.pools.clone()))
    }

    fn rule_request(&self) -> Arc<dyn RuleRequestRepository> {
        Arc::new(SqliteRuleRequestRepository::new_pools(self.pools.clone()))
    }

    fn push(&self) -> Arc<dyn PushRepository> {
        Arc::new(SqlitePushRepository::new_pools(self.pools.clone()))
    }

    fn notification(&self) -> Arc<dyn NotificationRepository> {
        Arc::new(SqliteNotificationRepository::new_pools(self.pools.clone()))
    }

    fn dumper(&self) -> Arc<dyn database_dumper::DatabaseDumper> {
        // `VACUUM INTO` acquires the writer lock briefly, so the
        // dumper is wired against the writer pool.
        Arc::new(database_dumper::SqliteDumper::new(
            self.pools.write.clone(),
            self.database_path.clone(),
        ))
    }
}
