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

use repository::{
    AdminRepository, ApiKeyRepository, DeviceRepository, DhcpRepository, DnsRepository,
    SessionRepository, SqliteAdminRepository, SqliteApiKeyRepository, SqliteDeviceRepository,
    SqliteDhcpRepository, SqliteDnsRepository, SqliteSessionRepository,
    SqliteSystemConfigRepository, SqliteTunnelRepository, SqliteUpdateRepository,
    SystemConfigRepository, TunnelRepository, UpdateRepository,
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
    fn update(&self) -> Arc<dyn UpdateRepository>;

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
pub struct SqliteRepositoryFactory {
    pool: SqlitePool,
    database_path: std::path::PathBuf,
}

impl SqliteRepositoryFactory {
    /// Initialise a new factory: open the connection pool against
    /// `connection_string`, run migrations, and bind everything to the
    /// factory instance. The production entry point called from
    /// [`create_repository_factory`].
    pub async fn connect(connection_string: &str) -> anyhow::Result<Self> {
        let pool = db::init_pool_from_connection_string(connection_string).await?;
        let database_path = std::path::PathBuf::from(connection_string);
        Ok(Self {
            pool,
            database_path,
        })
    }

    /// Construct from an already-initialised pool.
    ///
    /// Used by the mock (which pre-seeds data into an in-memory pool
    /// and needs to hand the populated pool to the service layer) and
    /// by tests. Not for production wiring — prefer
    /// [`Self::connect`], which keeps pool creation inside the
    /// factory's lifecycle.
    #[must_use]
    pub fn from_pool(pool: SqlitePool, database_path: std::path::PathBuf) -> Self {
        Self {
            pool,
            database_path,
        }
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

    fn update(&self) -> Arc<dyn UpdateRepository> {
        Arc::new(SqliteUpdateRepository::new(self.pool.clone()))
    }

    fn dumper(&self) -> Arc<dyn database_dumper::DatabaseDumper> {
        Arc::new(database_dumper::SqliteDumper::new(
            self.pool.clone(),
            self.database_path.clone(),
        ))
    }
}
