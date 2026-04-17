use async_trait::async_trait;
use sqlx::SqlitePool;

use super::super::ApiKeyRepository;

/// SQLite-backed implementation of [`ApiKeyRepository`].
pub struct SqliteApiKeyRepository {
    pool: SqlitePool,
}

impl SqliteApiKeyRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyRepository for SqliteApiKeyRepository {
    async fn find_all_hashes(&self) -> anyhow::Result<Vec<(String, String)>> {
        let rows = sqlx::query_as::<_, (String, String)>("SELECT id, key_hash FROM api_keys")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    async fn create(
        &self,
        id: &str,
        label: &str,
        key_hash: &str,
        created_at: &str,
    ) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO api_keys (id, label, key_hash, created_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(label)
            .bind(key_hash)
            .bind(created_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_last_used(&self, id: &str, now: &str) -> anyhow::Result<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = ? WHERE id = ?")
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
