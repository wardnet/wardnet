use async_trait::async_trait;
use sqlx::SqlitePool;

use super::super::SessionRepository;

/// SQLite-backed implementation of [`SessionRepository`].
pub struct SqliteSessionRepository {
    pool: SqlitePool,
}

impl SqliteSessionRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO sessions (id, admin_id, token_hash, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(admin_id)
        .bind(token_hash)
        .bind(created_at)
        .bind(expires_at)
        .execute(&self.pool)
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
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn delete_expired(&self, now: &str) -> anyhow::Result<u64> {
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
}
