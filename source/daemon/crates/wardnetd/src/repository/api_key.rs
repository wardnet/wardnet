use async_trait::async_trait;
use sqlx::SqlitePool;

/// Data access for API keys.
///
/// Stores argon2-hashed API keys. Listing returns hashes so the service layer
/// can verify incoming keys; this repository never sees plaintext keys.
#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    /// Return all `(id, key_hash)` pairs for verification.
    async fn find_all_hashes(&self) -> anyhow::Result<Vec<(String, String)>>;

    /// Insert a new API key row.
    async fn create(
        &self,
        id: &str,
        label: &str,
        key_hash: &str,
        created_at: &str,
    ) -> anyhow::Result<()>;

    /// Update the `last_used_at` timestamp for the given key.
    async fn update_last_used(&self, id: &str, now: &str) -> anyhow::Result<()>;
}

/// SQLite-backed implementation of [`ApiKeyRepository`].
pub struct SqliteApiKeyRepository {
    pool: SqlitePool,
}

impl SqliteApiKeyRepository {
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
