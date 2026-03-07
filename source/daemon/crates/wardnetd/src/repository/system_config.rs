use async_trait::async_trait;
use sqlx::SqlitePool;

/// Data access for the key-value `system_config` table and aggregate counts.
///
/// Provides simple get/set for configuration values and count queries for
/// devices and tunnels. Used by [`SystemService`](crate::service::SystemService)
/// to build status responses.
#[async_trait]
pub trait SystemConfigRepository: Send + Sync {
    /// Retrieve a config value by key.
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>>;

    /// Insert or update a config value.
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()>;

    /// Return the total number of rows in the `devices` table.
    async fn device_count(&self) -> anyhow::Result<i64>;

    /// Return the total number of rows in the `tunnels` table.
    async fn tunnel_count(&self) -> anyhow::Result<i64>;
}

/// SQLite-backed implementation of [`SystemConfigRepository`].
pub struct SqliteSystemConfigRepository {
    pool: SqlitePool,
}

impl SqliteSystemConfigRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SystemConfigRepository for SqliteSystemConfigRepository {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        let row = sqlx::query_scalar::<_, String>("SELECT value FROM system_config WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO system_config (key, value) VALUES (?, ?) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn device_count(&self) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM devices")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tunnels")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }
}
