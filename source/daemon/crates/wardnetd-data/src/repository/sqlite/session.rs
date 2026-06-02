use async_trait::async_trait;
use sqlx::SqlitePool;

use super::super::SessionRepository;
use crate::db::DbPools;

/// SQLite-backed implementation of [`SessionRepository`].
pub struct SqliteSessionRepository {
    pools: DbPools,
}

impl SqliteSessionRepository {
    /// Create a new repository backed by the given connection pool.
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
impl SessionRepository for SqliteSessionRepository {
    async fn create(
        &self,
        id: &str,
        admin_id: &str,
        token_hash: &str,
        created_at: &str,
        expires_at: &str,
        remember_me: bool,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO sessions (id, admin_id, token_hash, created_at, expires_at, remember_me) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(admin_id)
        .bind(token_hash)
        .bind(created_at)
        .bind(expires_at)
        .bind(remember_me)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn find_admin_id_by_token_hash(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<String>> {
        let row = sqlx::query_scalar::<_, String>(
            "SELECT admin_id FROM sessions WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pools.read)
        .await?;
        Ok(row)
    }

    async fn delete_expired(&self, now: &str) -> anyhow::Result<u64> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn extend_expiry(&self, token_hash: &str, new_expires_at: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE sessions SET expires_at = ? WHERE token_hash = ?")
            .bind(new_expires_at)
            .bind(token_hash)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn find_session_for_refresh(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<(String, bool)>> {
        let row = sqlx::query_as::<_, (String, bool)>(
            "SELECT admin_id, remember_me FROM sessions WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(&self.pools.read)
        .await?;
        Ok(row)
    }
}
