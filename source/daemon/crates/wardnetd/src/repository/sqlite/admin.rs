use async_trait::async_trait;
use sqlx::SqlitePool;

use super::super::AdminRepository;

/// SQLite-backed implementation of [`AdminRepository`].
pub struct SqliteAdminRepository {
    pool: SqlitePool,
}

impl SqliteAdminRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AdminRepository for SqliteAdminRepository {
    async fn find_by_username(&self, username: &str) -> anyhow::Result<Option<(String, String)>> {
        let row = sqlx::query_as::<_, (String, String)>(
            "SELECT id, password_hash FROM admins WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    async fn create(&self, id: &str, username: &str, password_hash: &str) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO admins (id, username, password_hash) VALUES (?, ?, ?)")
            .bind(id)
            .bind(username)
            .bind(password_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn find_first_id(&self) -> anyhow::Result<Option<String>> {
        let row = sqlx::query_scalar::<_, String>("SELECT id FROM admins LIMIT 1")
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    async fn exists(&self) -> anyhow::Result<bool> {
        let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM admins")
            .fetch_one(&self.pool)
            .await?;
        Ok(count > 0)
    }
}
