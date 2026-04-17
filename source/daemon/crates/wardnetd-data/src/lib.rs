pub mod bootstrap;
pub mod db;
pub mod keys;
pub mod oui;
pub mod repository;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use wardnet_common::config::{ApplicationConfiguration, DatabaseProvider};

use repository::{
    AdminRepository, ApiKeyRepository, DeviceRepository, DhcpRepository, DnsRepository,
    SessionRepository, SqliteAdminRepository, SqliteApiKeyRepository, SqliteDeviceRepository,
    SqliteDhcpRepository, SqliteDnsRepository, SqliteSessionRepository,
    SqliteSystemConfigRepository, SqliteTunnelRepository, SystemConfigRepository, TunnelRepository,
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
    fn system_config(&self) -> Arc<dyn SystemConfigRepository>;
    fn dhcp(&self) -> Arc<dyn DhcpRepository>;
    fn dns(&self) -> Arc<dyn DnsRepository>;
    fn tunnel(&self) -> Arc<dyn TunnelRepository>;
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
            let pool =
                db::init_pool_from_connection_string(&config.database.connection_string).await?;
            Ok(Box::new(SqliteRepositoryFactory::new(pool)))
        }
    }
}

/// `SQLite`-backed repository factory.
pub struct SqliteRepositoryFactory {
    pool: SqlitePool,
}

impl SqliteRepositoryFactory {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl RepositoryFactory for SqliteRepositoryFactory {
    fn admin(&self) -> Arc<dyn AdminRepository> {
        Arc::new(SqliteAdminRepository::new(self.pool.clone()))
    }

    fn session(&self) -> Arc<dyn SessionRepository> {
        Arc::new(SqliteSessionRepository::new(self.pool.clone()))
    }

    fn api_key(&self) -> Arc<dyn ApiKeyRepository> {
        Arc::new(SqliteApiKeyRepository::new(self.pool.clone()))
    }

    fn device(&self) -> Arc<dyn DeviceRepository> {
        Arc::new(SqliteDeviceRepository::new(self.pool.clone()))
    }

    fn system_config(&self) -> Arc<dyn SystemConfigRepository> {
        Arc::new(SqliteSystemConfigRepository::new(self.pool.clone()))
    }

    fn dhcp(&self) -> Arc<dyn DhcpRepository> {
        Arc::new(SqliteDhcpRepository::new(self.pool.clone()))
    }

    fn dns(&self) -> Arc<dyn DnsRepository> {
        Arc::new(SqliteDnsRepository::new(self.pool.clone()))
    }

    fn tunnel(&self) -> Arc<dyn TunnelRepository> {
        Arc::new(SqliteTunnelRepository::new(self.pool.clone()))
    }
}
