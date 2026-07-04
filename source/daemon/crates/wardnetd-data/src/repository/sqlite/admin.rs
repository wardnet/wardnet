use async_trait::async_trait;
use sqlx::SqlitePool;

use super::super::AdminRepository;
use crate::db::DbPools;

/// SQLite-backed implementation of [`AdminRepository`].
pub struct SqliteAdminRepository {
    pools: DbPools,
}

impl SqliteAdminRepository {
    /// Create a new repository backed by the given connection pool.
    /// Reads and writes share the pool (used by tests and the
    /// in-memory mock).
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    /// Create a new repository with split reader/writer pools.
    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl AdminRepository for SqliteAdminRepository {
    async fn find_by_username(&self, username: &str) -> anyhow::Result<Option<(String, String)>> {
        let row = sqlx::query_as::<_, (String, String)>(
            "SELECT id, password_hash FROM admins WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pools.read)
        .await?;
        Ok(row)
    }

    async fn create(&self, id: &str, username: &str, password_hash: &str) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO admins (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(id)
            .bind(username)
            .bind(password_hash)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn find_first_id(&self) -> anyhow::Result<Option<String>> {
        let row = sqlx::query_scalar::<_, String>("SELECT id FROM admins LIMIT 1")
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row)
    }

    async fn find_username_by_id(&self, id: &str) -> anyhow::Result<Option<String>> {
        let row = sqlx::query_scalar::<_, String>("SELECT username FROM admins WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row)
    }

    async fn exists(&self) -> anyhow::Result<bool> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM admins")
            .fetch_one(&self.pools.read)
            .await?;
        Ok(count > 0)
    }
}
