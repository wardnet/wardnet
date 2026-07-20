//! SQLite-backed [`MaintenanceRepository`] implementation.

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::maintenance::{MaintenanceRepository, WalCheckpointOutcome};

/// How many pages a single `PRAGMA incremental_vacuum(N)` call is
/// allowed to reclaim. Tuned for a Raspberry Pi: ~8 MiB at the `SQLite`
/// default 4 KiB page size — small enough to release the writer lock
/// promptly so concurrent flushes don't queue against
/// `busy_timeout`. The cleanup tick fires daily; unreclaimed pages
/// stay on the freelist and get picked up by the next call.
const INCREMENTAL_VACUUM_PAGES: u32 = 2_000;

/// Upper bound on how many rows-per-index `PRAGMA optimize` samples
/// before it stops, per `SQLite`'s own recommendation for keeping the
/// analysis cheap on large tables. Without a limit, `ANALYZE` on a table
/// like `dns_query_log` can scan the whole index and hold the writer for
/// seconds; 400 gives the planner good-enough estimates in milliseconds.
const ANALYSIS_LIMIT: u32 = 400;

pub struct SqliteMaintenanceRepository {
    pools: DbPools,
}

impl SqliteMaintenanceRepository {
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl MaintenanceRepository for SqliteMaintenanceRepository {
    async fn incremental_vacuum(&self) -> anyhow::Result<u64> {
        // `PRAGMA incremental_vacuum(N)` is a no-op on databases
        // created with `auto_vacuum=NONE`, so it's safe to call
        // unconditionally. The `before`/`after` freelist samples are
        // taken outside any transaction; concurrent writers can grow
        // the freelist between samples, so the returned page count is
        // best-effort telemetry rather than a precise total.
        let before: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(&self.pools.write)
            .await
            .unwrap_or(0);
        let stmt = format!("PRAGMA incremental_vacuum({INCREMENTAL_VACUUM_PAGES})");
        sqlx::query(sqlx::AssertSqlSafe(stmt))
            .execute(&self.pools.write)
            .await?;
        let after: i64 = sqlx::query_scalar("PRAGMA freelist_count")
            .fetch_one(&self.pools.write)
            .await
            .unwrap_or(before);
        Ok(u64::try_from((before - after).max(0)).unwrap_or(0))
    }

    async fn wal_checkpoint_truncate(&self) -> anyhow::Result<WalCheckpointOutcome> {
        // `PRAGMA wal_checkpoint(TRUNCATE)` returns exactly one row:
        // `(busy, log, checkpointed)`. `busy = 1` means a reader still
        // held a snapshot of some WAL frames, so the file could not be
        // truncated this pass — reported, not raised, so the daily runner
        // simply retries on the next tick. Runs on the writer connection:
        // a checkpoint coordinates with the same lock the writer holds,
        // and TRUNCATE must not race a second writer.
        let (busy, wal_frames, checkpointed_frames): (i64, i64, i64) =
            sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
                .fetch_one(&self.pools.write)
                .await?;
        Ok(WalCheckpointOutcome {
            busy: busy != 0,
            wal_frames,
            checkpointed_frames,
        })
    }

    async fn optimize(&self) -> anyhow::Result<()> {
        // Bound the work first (see `ANALYSIS_LIMIT`), then let
        // `PRAGMA optimize` decide which tables actually need `ANALYZE`.
        // Both run on the writer since `optimize` may write `sqlite_stat1`.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA analysis_limit = {ANALYSIS_LIMIT}"
        )))
        .execute(&self.pools.write)
        .await?;
        sqlx::query("PRAGMA optimize")
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn ping(&self) -> anyhow::Result<()> {
        // Read pool: a health probe must never contend for the single
        // writer connection. `SELECT 1` is a const string so it allocates
        // nothing per call.
        const PING: &str = "SELECT 1";
        let _: i64 = sqlx::query_scalar(PING).fetch_one(&self.pools.read).await?;
        Ok(())
    }
}
