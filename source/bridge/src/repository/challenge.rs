use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

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

/// Raw `PostgreSQL` row for `sqlx::query_as` mapping.
///
/// `difficulty` is stored as `INTEGER` (Postgres has no unsigned types) and
/// converted to the domain `u32` at the boundary — never with `as`.
#[derive(sqlx::FromRow)]
struct ChallengeRow {
    id: String,
    nonce: String,
    difficulty: i32,
    remote_ip: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    used_at: Option<DateTime<Utc>>,
}

impl ChallengeRow {
    fn into_challenge(self) -> anyhow::Result<RegistrationChallenge> {
        let difficulty = u32::try_from(self.difficulty).map_err(|_| {
            anyhow::anyhow!(
                "difficulty for challenge {} is negative: {}",
                self.id,
                self.difficulty
            )
        })?;
        Ok(RegistrationChallenge {
            id: self.id,
            nonce: self.nonce,
            difficulty,
            remote_ip: self.remote_ip,
            created_at: self.created_at,
            expires_at: self.expires_at,
            used_at: self.used_at,
        })
    }
}

const FIND_BY_ID: &str = "SELECT id, nonce, difficulty, remote_ip, created_at, expires_at, used_at \
     FROM registration_challenges WHERE id = $1";

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

/// PostgreSQL-backed [`ChallengeRepository`].
pub struct PgChallengeRepository {
    pools: DbPools,
}

impl PgChallengeRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
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
impl ChallengeRepository for PgChallengeRepository {
    async fn insert(&self, c: &RegistrationChallenge) -> anyhow::Result<()> {
        // Postgres has no unsigned integers — convert the domain `u32` to the
        // stored `INTEGER` explicitly at the boundary (never `as`).
        let difficulty = i32::try_from(c.difficulty)
            .map_err(|_| anyhow::anyhow!("difficulty {} exceeds i32::MAX", c.difficulty))?;
        sqlx::query(
            "INSERT INTO registration_challenges
             (id, nonce, difficulty, remote_ip, created_at, expires_at, used_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&c.id)
        .bind(&c.nonce)
        .bind(difficulty)
        .bind(&c.remote_ip)
        .bind(c.created_at)
        .bind(c.expires_at)
        .bind(c.used_at)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<RegistrationChallenge>> {
        sqlx::query_as::<_, ChallengeRow>(FIND_BY_ID)
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?
            .map(ChallengeRow::into_challenge)
            .transpose()
    }

    async fn consume(&self, id: &str, used_at: DateTime<Utc>) -> anyhow::Result<bool> {
        let rows = sqlx::query(
            "UPDATE registration_challenges
             SET used_at = $1
             WHERE id = $2 AND used_at IS NULL",
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
             WHERE remote_ip = $1 AND created_at > $2",
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
