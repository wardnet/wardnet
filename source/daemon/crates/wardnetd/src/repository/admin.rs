use async_trait::async_trait;
use sqlx::SqlitePool;

/// Data access for admin accounts.
///
/// Provides CRUD operations for the `admins` table. No business logic —
/// validation, password hashing, and policy checks belong in the service layer.
#[async_trait]
pub trait AdminRepository: Send + Sync {
    /// Look up an admin by username, returning `(id, password_hash)` if found.
    async fn find_by_username(&self, username: &str) -> anyhow::Result<Option<(String, String)>>;

    /// Insert a new admin row.
    async fn create(&self, id: &str, username: &str, password_hash: &str) -> anyhow::Result<()>;

    /// Return the id of the first admin (used for single-admin MVP).
    async fn find_first_id(&self) -> anyhow::Result<Option<String>>;
}

/// SQLite-backed implementation of [`AdminRepository`].
pub struct SqliteAdminRepository {
    pool: SqlitePool,
}

impl SqliteAdminRepository {
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
}
