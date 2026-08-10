use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::session::{
    SessionForRefresh, SessionPrincipal, SessionRepository, SessionSummary,
};
use crate::repository::user::UserRole;

const INSERT: &str = "INSERT INTO sessions \
     (id, user_id, token_hash, created_at, expires_at, remember_me, \
      device_id, user_agent, absolute_expires_at) \
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// The authentication join. Four conditions — token match, sliding expiry,
/// absolute ceiling, and `u.enabled = 1` — all live in SQL so no caller can
/// forget one. Disabling a user therefore takes effect on their very next
/// request, without hunting down their sessions first.
const FIND_PRINCIPAL: &str = "SELECT s.user_id, u.role, u.display_name \
     FROM sessions s \
     JOIN users u ON u.id = s.user_id \
     WHERE s.token_hash = ? \
       AND s.expires_at > ? \
       AND s.absolute_expires_at > ? \
       AND u.enabled = 1";

const DELETE_EXPIRED: &str = "DELETE FROM sessions WHERE expires_at <= ?";

const DELETE_BY_TOKEN_HASH: &str = "DELETE FROM sessions WHERE token_hash = ?";

const DELETE_BY_ID: &str = "DELETE FROM sessions WHERE id = ? AND user_id = ?";

const DELETE_ALL_FOR_USER: &str = "DELETE FROM sessions WHERE user_id = ?";

/// Same liveness predicate as `FIND_PRINCIPAL`. A session past its hard
/// ceiling is dead for authentication, so listing it as live would show the
/// user a session whose revocation appears to do nothing.
const LIST_FOR_USER: &str = "SELECT id, user_id, device_id, user_agent, created_at, expires_at \
     FROM sessions \
     WHERE user_id = ? AND expires_at > ? AND absolute_expires_at > ? \
     ORDER BY created_at DESC";

/// Rotation never touches `absolute_expires_at`: the ceiling is set once, at
/// issue time, and a refresh that could raise it would not be a ceiling.
const ROTATE_TOKEN: &str =
    "UPDATE sessions SET token_hash = ?, expires_at = ? WHERE token_hash = ?";

/// Refresh must apply **exactly** the same four conditions as `FIND_PRINCIPAL`.
/// Filtering on `token_hash` and `expires_at` alone would let a disabled user's
/// client keep rotating itself a fresh token indefinitely — a 200 on refresh
/// and a 403 on everything else — and would let a session past its absolute
/// ceiling be extended past it, which is precisely what a ceiling is for.
const FIND_FOR_REFRESH: &str = "SELECT s.user_id, s.remember_me, s.created_at, s.absolute_expires_at \
     FROM sessions s \
     JOIN users u ON u.id = s.user_id \
     WHERE s.token_hash = ? \
       AND s.expires_at > ? \
       AND s.absolute_expires_at > ? \
       AND u.enabled = 1";

#[derive(sqlx::FromRow)]
struct DbPrincipalRow {
    user_id: String,
    role: String,
    display_name: String,
}

#[derive(sqlx::FromRow)]
struct DbSessionSummaryRow {
    id: String,
    user_id: String,
    device_id: Option<String>,
    user_agent: Option<String>,
    created_at: String,
    expires_at: String,
}

#[derive(sqlx::FromRow)]
struct DbSessionForRefreshRow {
    user_id: String,
    remember_me: bool,
    created_at: String,
    absolute_expires_at: String,
}

/// `SQLite`-backed implementation of [`SessionRepository`].
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
        user_id: &str,
        token_hash: &str,
        created_at: &str,
        expires_at: &str,
        remember_me: bool,
        device_id: Option<&str>,
        user_agent: Option<&str>,
        absolute_expires_at: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(INSERT)
            .bind(id)
            .bind(user_id)
            .bind(token_hash)
            .bind(created_at)
            .bind(expires_at)
            .bind(remember_me)
            .bind(device_id)
            .bind(user_agent)
            .bind(absolute_expires_at)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn find_principal_by_token_hash(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<SessionPrincipal>> {
        let row = sqlx::query_as::<_, DbPrincipalRow>(FIND_PRINCIPAL)
            .bind(token_hash)
            .bind(now)
            .bind(now)
            .fetch_optional(&self.pools.read)
            .await?;

        let Some(row) = row else { return Ok(None) };
        let role = UserRole::parse(&row.role).ok_or_else(|| {
            anyhow::anyhow!("user {} has unrecognised role {:?}", row.user_id, row.role)
        })?;
        Ok(Some(SessionPrincipal {
            user_id: row.user_id,
            role,
            display_name: row.display_name,
        }))
    }

    async fn delete_expired(&self, now: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(DELETE_EXPIRED)
            .bind(now)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn delete_by_token_hash(&self, token_hash: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(DELETE_BY_TOKEN_HASH)
            .bind(token_hash)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn delete_by_id(&self, id: &str, user_id: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(DELETE_BY_ID)
            .bind(id)
            .bind(user_id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn delete_all_for_user(&self, user_id: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(DELETE_ALL_FOR_USER)
            .bind(user_id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn list_for_user(&self, user_id: &str, now: &str) -> anyhow::Result<Vec<SessionSummary>> {
        let rows = sqlx::query_as::<_, DbSessionSummaryRow>(LIST_FOR_USER)
            .bind(user_id)
            .bind(now)
            .bind(now)
            .fetch_all(&self.pools.read)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| SessionSummary {
                id: r.id,
                user_id: r.user_id,
                device_id: r.device_id,
                user_agent: r.user_agent,
                created_at: r.created_at,
                expires_at: r.expires_at,
            })
            .collect())
    }

    async fn rotate_token(
        &self,
        old_token_hash: &str,
        new_token_hash: &str,
        new_expires_at: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(ROTATE_TOKEN)
            .bind(new_token_hash)
            .bind(new_expires_at)
            .bind(old_token_hash)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn find_session_for_refresh(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<SessionForRefresh>> {
        let row = sqlx::query_as::<_, DbSessionForRefreshRow>(FIND_FOR_REFRESH)
            .bind(token_hash)
            .bind(now)
            .bind(now)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row.map(|r| SessionForRefresh {
            user_id: r.user_id,
            remember_me: r.remember_me,
            created_at: r.created_at,
            absolute_expires_at: r.absolute_expires_at,
        }))
    }
}
