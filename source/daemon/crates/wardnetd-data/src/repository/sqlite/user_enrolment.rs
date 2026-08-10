use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::user_enrolment::{EnrolmentTokenRow, UserEnrolmentRepository};

#[derive(sqlx::FromRow)]
struct DbEnrolmentTokenRow {
    id: String,
    user_id: String,
    token_hash: String,
    created_at: String,
    expires_at: String,
    used_at: Option<String>,
}

impl DbEnrolmentTokenRow {
    fn into_domain(self) -> EnrolmentTokenRow {
        EnrolmentTokenRow {
            id: self.id,
            user_id: self.user_id,
            token_hash: self.token_hash,
            created_at: self.created_at,
            expires_at: self.expires_at,
            used_at: self.used_at,
        }
    }
}

const INSERT: &str = "INSERT INTO user_enrolment_tokens \
     (id, user_id, token_hash, created_at, expires_at, used_at) \
     VALUES (?, ?, ?, ?, ?, ?)";

/// All three redeemability conditions live here rather than in the service, so
/// a caller cannot redeem a spent or expired token by forgetting a check.
const FIND_REDEEMABLE: &str = "SELECT id, user_id, token_hash, created_at, expires_at, used_at \
     FROM user_enrolment_tokens \
     WHERE token_hash = ? AND expires_at > ? AND used_at IS NULL";

/// `used_at IS NULL` is in the `WHERE` clause on purpose: it makes redemption
/// a compare-and-swap, so two concurrent redemptions cannot both report
/// success. A check-then-write in the service would race.
const MARK_USED: &str =
    "UPDATE user_enrolment_tokens SET used_at = ? WHERE id = ? AND used_at IS NULL";

const LIST_FOR_USER: &str = "SELECT id, user_id, token_hash, created_at, expires_at, used_at \
     FROM user_enrolment_tokens WHERE user_id = ? ORDER BY created_at DESC";

const DELETE: &str = "DELETE FROM user_enrolment_tokens WHERE id = ?";

const DELETE_EXPIRED: &str = "DELETE FROM user_enrolment_tokens WHERE expires_at <= ?";

/// `SQLite`-backed [`UserEnrolmentRepository`].
pub struct SqliteUserEnrolmentRepository {
    pools: DbPools,
}

impl SqliteUserEnrolmentRepository {
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
impl UserEnrolmentRepository for SqliteUserEnrolmentRepository {
    async fn create(&self, row: &EnrolmentTokenRow) -> anyhow::Result<()> {
        sqlx::query(INSERT)
            .bind(&row.id)
            .bind(&row.user_id)
            .bind(&row.token_hash)
            .bind(&row.created_at)
            .bind(&row.expires_at)
            .bind(row.used_at.as_deref())
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn find_redeemable(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<EnrolmentTokenRow>> {
        let row = sqlx::query_as::<_, DbEnrolmentTokenRow>(FIND_REDEEMABLE)
            .bind(token_hash)
            .bind(now)
            .fetch_optional(&self.pools.read)
            .await?;
        Ok(row.map(DbEnrolmentTokenRow::into_domain))
    }

    async fn mark_used(&self, id: &str, now: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(MARK_USED)
            .bind(now)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn list_for_user(&self, user_id: &str) -> anyhow::Result<Vec<EnrolmentTokenRow>> {
        let rows = sqlx::query_as::<_, DbEnrolmentTokenRow>(LIST_FOR_USER)
            .bind(user_id)
            .fetch_all(&self.pools.read)
            .await?;
        Ok(rows
            .into_iter()
            .map(DbEnrolmentTokenRow::into_domain)
            .collect())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(DELETE)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn delete_expired(&self, now: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(DELETE_EXPIRED)
            .bind(now)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }
}
