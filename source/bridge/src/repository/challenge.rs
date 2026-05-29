use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::MySqlPool;

use crate::db::DbPools;

/// A single-use `PoW` challenge gating `POST /v1/register`.
#[derive(Debug, Clone)]
pub struct RegistrationChallenge {
    pub id: String,
    /// 32 random bytes encoded as lowercase hex.
    pub nonce: String,
    /// Required number of leading zero bits in
    /// `SHA256(nonce\nname\npublic_key\nproof)`.
    pub difficulty: u32,
    pub remote_ip: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Set atomically when the challenge is consumed by a registration.
    pub used_at: Option<DateTime<Utc>>,
}

/// Raw `MySQL` row for `sqlx::query_as` mapping.
#[derive(sqlx::FromRow)]
struct ChallengeRow {
    id: String,
    nonce: String,
    difficulty: u32,
    remote_ip: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    used_at: Option<DateTime<Utc>>,
}

impl ChallengeRow {
    fn into_challenge(self) -> RegistrationChallenge {
        RegistrationChallenge {
            id: self.id,
            nonce: self.nonce,
            difficulty: self.difficulty,
            remote_ip: self.remote_ip,
            created_at: self.created_at,
            expires_at: self.expires_at,
            used_at: self.used_at,
        }
    }
}

const FIND_BY_ID: &str = "SELECT id, nonce, difficulty, remote_ip, created_at, expires_at, used_at \
     FROM registration_challenges WHERE id = ?";

/// Data access for `registration_challenges`.
#[async_trait]
pub trait ChallengeRepository: Send + Sync {
    /// Persist a newly-issued challenge.
    async fn insert(&self, challenge: &RegistrationChallenge) -> anyhow::Result<()>;

    /// Find a challenge by its UUID.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<RegistrationChallenge>>;

    /// Atomically mark a challenge as used.
    ///
    /// Updates `used_at` only when `used_at IS NULL` (i.e. not yet consumed).
    /// Returns `true` if the row was updated, `false` if it was already used
    /// or does not exist.
    async fn consume(&self, id: &str, used_at: DateTime<Utc>) -> anyhow::Result<bool>;

    /// Count how many challenges have been issued to `remote_ip` since `since`.
    /// Used for the per-IP challenge rate limit.
    async fn count_from_ip(&self, remote_ip: &str, since: DateTime<Utc>) -> anyhow::Result<i64>;
}

/// MySQL-backed [`ChallengeRepository`].
pub struct MySqlChallengeRepository {
    pools: DbPools,
}

impl MySqlChallengeRepository {
    #[must_use]
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            pools: DbPools::single(pool),
        }
    }

    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl ChallengeRepository for MySqlChallengeRepository {
    async fn insert(&self, c: &RegistrationChallenge) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO registration_challenges
             (id, nonce, difficulty, remote_ip, created_at, expires_at, used_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&c.id)
        .bind(&c.nonce)
        .bind(c.difficulty)
        .bind(&c.remote_ip)
        .bind(c.created_at)
        .bind(c.expires_at)
        .bind(c.used_at)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<RegistrationChallenge>> {
        Ok(sqlx::query_as::<_, ChallengeRow>(FIND_BY_ID)
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?
            .map(ChallengeRow::into_challenge))
    }

    async fn consume(&self, id: &str, used_at: DateTime<Utc>) -> anyhow::Result<bool> {
        let rows = sqlx::query(
            "UPDATE registration_challenges
             SET used_at = ?
             WHERE id = ? AND used_at IS NULL",
        )
        .bind(used_at)
        .bind(id)
        .execute(&self.pools.write)
        .await?
        .rows_affected();
        Ok(rows > 0)
    }

    async fn count_from_ip(&self, remote_ip: &str, since: DateTime<Utc>) -> anyhow::Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM registration_challenges
             WHERE remote_ip = ? AND created_at > ?",
        )
        .bind(remote_ip)
        .bind(since)
        .fetch_one(&self.pools.read)
        .await?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests;
