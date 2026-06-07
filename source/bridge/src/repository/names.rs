use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::db::DbPools;

const RESERVE: &str = "INSERT INTO names (slug, install_id, region, status, expires_at, created_at) \
     VALUES ($1, $2, $3, 'reserved', $4, $5)";

const CONFIRM: &str = "UPDATE names SET status = 'active', expires_at = NULL WHERE slug = $1";

const RELEASE: &str = "DELETE FROM names WHERE slug = $1 AND status = 'reserved'";

const IS_TAKEN: &str = "SELECT EXISTS(SELECT 1 FROM names WHERE slug = $1)";

const SWEEP_EXPIRED: &str = "DELETE FROM names \
     WHERE status = 'reserved' AND expires_at < $1 AND region = $2 \
     RETURNING install_id";

/// The global naming authority: allocation of vanity slugs across the bridge
/// fleet's flat global namespace.
///
/// Backed by a **separate** global Postgres (distinct from each bridge's
/// regional install DB). The `slug` PRIMARY KEY is the cross-region allocation
/// lock — [`reserve`](NameRepository::reserve) returns `Ok(false)` on a unique
/// violation, which is how a name-clash is detected atomically.
///
/// Registration is two-phase: `reserve` → `confirm`, with `release` as the
/// compensating action and `sweep_expired` reaping abandoned reservations. The
/// install row lives in the regional DB, so callers compensate **both**
/// databases (see `crate::sweep` and the register handler).
#[async_trait]
pub trait NameRepository: Send + Sync {
    /// Atomically reserve `slug` for `install_id` in `region`.
    ///
    /// Inserts a `reserved` row created at `created_at` and expiring at
    /// `expires_at`. Returns `Ok(true)` when the name was ours to take,
    /// `Ok(false)` when the slug is already taken (unique violation on the
    /// PRIMARY KEY — the allocation lock).
    async fn reserve(
        &self,
        slug: &str,
        install_id: &str,
        region: &str,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> anyhow::Result<bool>;

    /// Promote a reserved slug to `active`, clearing its expiry. Errors if no
    /// row matched (the reservation vanished — e.g. swept — between provision
    /// and confirm), so the caller never reports success for a name that isn't
    /// actually allocated.
    async fn confirm(&self, slug: &str) -> anyhow::Result<()>;

    /// Release a reservation (compensating action). Deletes the row only while
    /// it is still `reserved`, so a confirmed name is never dropped by a late
    /// error on the registration path. Returns `true` when a reserved row was
    /// actually removed — the caller pairs the regional install deletion to this,
    /// so a name that turned out to be `active` (e.g. a confirm whose ack was
    /// lost) keeps its install row rather than being half-deleted.
    async fn release(&self, slug: &str) -> anyhow::Result<bool>;

    /// Whether `slug` currently has any row (reserved or active). An abandoned
    /// reservation reads as taken until [`sweep_expired`](Self::sweep_expired)
    /// removes it, so availability and [`reserve`](Self::reserve) always agree.
    async fn is_taken(&self, slug: &str) -> anyhow::Result<bool>;

    /// Delete expired `reserved` rows for `region`, returning their
    /// `install_id`s so the caller can clean the matching regional install rows.
    ///
    /// Region-scoped on purpose: each bridge owns the cleanup of its own
    /// region's reservations end-to-end (the regional orphan it leaked is only
    /// reachable by that bridge).
    async fn sweep_expired(&self, now: DateTime<Utc>, region: &str) -> anyhow::Result<Vec<String>>;
}

/// PostgreSQL-backed [`NameRepository`] against the global pool.
pub struct PgNameRepository {
    pools: DbPools,
}

impl PgNameRepository {
    /// Create a repository backed by a single pool (tests).
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pools: DbPools::single(pool),
        }
    }

    /// Create a repository with split reader / writer pools.
    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl NameRepository for PgNameRepository {
    async fn reserve(
        &self,
        slug: &str,
        install_id: &str,
        region: &str,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(RESERVE)
            .bind(slug)
            .bind(install_id)
            .bind(region)
            .bind(expires_at)
            .bind(created_at)
            .execute(&self.pools.write)
            .await;

        match result {
            Ok(_) => Ok(true),
            // A unique violation on the slug PRIMARY KEY is the allocation lock
            // firing: the name is already taken. Every other error is a real
            // failure and propagates.
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn confirm(&self, slug: &str) -> anyhow::Result<()> {
        let result = sqlx::query(CONFIRM)
            .bind(slug)
            .execute(&self.pools.write)
            .await?;
        if result.rows_affected() == 0 {
            anyhow::bail!("reservation for '{slug}' vanished before confirm (swept or released)");
        }
        Ok(())
    }

    async fn release(&self, slug: &str) -> anyhow::Result<bool> {
        let result = sqlx::query(RELEASE)
            .bind(slug)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn is_taken(&self, slug: &str) -> anyhow::Result<bool> {
        let exists: bool = sqlx::query_scalar(IS_TAKEN)
            .bind(slug)
            .fetch_one(&self.pools.read)
            .await?;
        Ok(exists)
    }

    async fn sweep_expired(&self, now: DateTime<Utc>, region: &str) -> anyhow::Result<Vec<String>> {
        let ids: Vec<String> = sqlx::query_scalar(SWEEP_EXPIRED)
            .bind(now)
            .bind(region)
            .fetch_all(&self.pools.write)
            .await?;
        Ok(ids)
    }
}
