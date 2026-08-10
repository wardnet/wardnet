use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::user::{DuplicateUserEmailError, UserRepository, UserRole, UserRow};

#[derive(sqlx::FromRow)]
struct DbUserRow {
    id: String,
    display_name: String,
    email: Option<String>,
    role: String,
    enabled: bool,
    created_at: String,
    updated_at: String,
}

impl DbUserRow {
    /// Map a database row to the domain type.
    ///
    /// An unparseable `role` is an error, never a default. The `CHECK`
    /// constraint makes it unreachable for rows this daemon wrote, but
    /// silently treating an unknown role as `Admin` would turn a hand-edited
    /// database into a privilege escalation.
    fn into_domain(self) -> anyhow::Result<UserRow> {
        let role = UserRole::parse(&self.role).ok_or_else(|| {
            anyhow::anyhow!("user {} has unrecognised role {:?}", self.id, self.role)
        })?;
        Ok(UserRow {
            id: self.id,
            display_name: self.display_name,
            email: self.email,
            role,
            enabled: self.enabled,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

/// Column list shared by every `SELECT` so the mapping stays in one place.
/// Spelled out inside each `const` query rather than interpolated: if every
/// component is constant, the whole statement should be constant
/// (`.agents/code-conventions.md`).
const FIND_BY_ID: &str = "SELECT id, display_name, email, role, enabled, created_at, updated_at \
     FROM users WHERE id = ?";

const FIND_BY_EMAIL: &str = "SELECT id, display_name, email, role, enabled, created_at, updated_at \
     FROM users WHERE lower(email) = lower(?)";

const FIND_ALL: &str = "SELECT id, display_name, email, role, enabled, created_at, updated_at \
     FROM users ORDER BY display_name COLLATE NOCASE";

const INSERT: &str = "INSERT INTO users \
     (id, display_name, email, role, enabled, created_at, updated_at) \
     VALUES (?, ?, ?, ?, ?, ?, ?)";

const UPDATE_PROFILE: &str =
    "UPDATE users SET display_name = ?, email = ?, updated_at = ? WHERE id = ?";

const SET_ENABLED: &str = "UPDATE users SET enabled = ?, updated_at = ? WHERE id = ?";

const SET_ROLE: &str = "UPDATE users SET role = ?, updated_at = ? WHERE id = ?";

const DELETE: &str = "DELETE FROM users WHERE id = ?";

const COUNT_ENABLED_ADMINS: &str =
    "SELECT COUNT(*) FROM users WHERE role = 'admin' AND enabled = 1";

const EXISTS: &str = "SELECT EXISTS(SELECT 1 FROM users)";

/// Ordered by `created_at` then `id` so the answer is deterministic even if two
/// admins share a timestamp — an API key must not silently change whose
/// identity it acts under between two reads.
const FIND_FIRST_ENABLED_ADMIN: &str =
    "SELECT id, display_name, email, role, enabled, created_at, updated_at \
     FROM users WHERE role = 'admin' AND enabled = 1 \
     ORDER BY created_at, id LIMIT 1";

/// Return `true` when a `SQLite` error is the case-insensitive email index
/// firing.
///
/// `SQLite` does not reliably populate `constraint()` with the index name, so
/// the dependable signal is the rendered message, which names the index. We
/// check `constraint()` first in case a future driver does populate it.
fn is_duplicate_email(db_err: &dyn sqlx::error::DatabaseError) -> bool {
    if db_err.constraint() == Some("idx_users_email_lower") {
        return true;
    }
    db_err.message().contains("idx_users_email_lower")
}

/// Re-wrap a `SQLite` error as [`DuplicateUserEmailError`] when it is the
/// email index, and pass everything else through unchanged.
fn map_email_conflict(err: sqlx::Error) -> anyhow::Error {
    if let sqlx::Error::Database(ref db_err) = err
        && is_duplicate_email(db_err.as_ref())
    {
        return anyhow::Error::new(DuplicateUserEmailError);
    }
    anyhow::Error::new(err)
}

/// `SQLite`-backed [`UserRepository`].
pub struct SqliteUserRepository {
    pools: DbPools,
}

impl SqliteUserRepository {
    /// Create a new repository backed by the given connection pool. Reads and
    /// writes share the pool (used by tests and the in-memory mock).
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
impl UserRepository for SqliteUserRepository {
    async fn create(&self, row: &UserRow) -> anyhow::Result<()> {
        sqlx::query(INSERT)
            .bind(&row.id)
            .bind(&row.display_name)
            .bind(row.email.as_deref())
            .bind(row.role.as_str())
            .bind(row.enabled)
            .bind(&row.created_at)
            .bind(&row.updated_at)
            .execute(&self.pools.write)
            .await
            .map_err(map_email_conflict)?;
        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<UserRow>> {
        let row = sqlx::query_as::<_, DbUserRow>(FIND_BY_ID)
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DbUserRow::into_domain).transpose()
    }

    async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<UserRow>> {
        let row = sqlx::query_as::<_, DbUserRow>(FIND_BY_EMAIL)
            .bind(email)
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DbUserRow::into_domain).transpose()
    }

    async fn find_all(&self) -> anyhow::Result<Vec<UserRow>> {
        let rows = sqlx::query_as::<_, DbUserRow>(FIND_ALL)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(DbUserRow::into_domain).collect()
    }

    async fn update_profile(
        &self,
        id: &str,
        display_name: &str,
        email: Option<&str>,
        now: &str,
    ) -> anyhow::Result<u64> {
        let result = sqlx::query(UPDATE_PROFILE)
            .bind(display_name)
            .bind(email)
            .bind(now)
            .bind(id)
            .execute(&self.pools.write)
            .await
            .map_err(map_email_conflict)?;
        Ok(result.rows_affected())
    }

    async fn set_enabled(&self, id: &str, enabled: bool, now: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(SET_ENABLED)
            .bind(enabled)
            .bind(now)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn set_role(&self, id: &str, role: UserRole, now: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(SET_ROLE)
            .bind(role.as_str())
            .bind(now)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(DELETE)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn count_enabled_admins(&self) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(COUNT_ENABLED_ADMINS)
            .fetch_one(&self.pools.read)
            .await?;
        Ok(count)
    }

    async fn exists(&self) -> anyhow::Result<bool> {
        let exists = sqlx::query_scalar::<_, bool>(EXISTS)
            .fetch_one(&self.pools.read)
            .await?;
        Ok(exists)
    }

    async fn find_first_enabled_admin(&self) -> anyhow::Result<Option<UserRow>> {
        let row = sqlx::query_as::<_, DbUserRow>(FIND_FIRST_ENABLED_ADMIN)
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DbUserRow::into_domain).transpose()
    }
}
