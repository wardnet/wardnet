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

/// Busy timeout (ms) the daily `wal_checkpoint(TRUNCATE)` runs under. A
/// TRUNCATE checkpoint waits for readers to release their WAL snapshots,
/// and the pool-wide `busy_timeout` is 30 s — far too long to let a
/// daily maintenance tick monopolise the single writer connection while a
/// long-lived reader is active. Bound the checkpoint's own wait to 1 s so
/// it either truncates promptly or reports `busy` and lets the next daily
/// tick retry; `journal_size_limit` keeps the file capped meanwhile.
const CHECKPOINT_BUSY_TIMEOUT_MS: u32 = 1_000;

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
        //
        // Acquire a single connection and drop its busy timeout for the
        // duration of the checkpoint so a TRUNCATE stuck behind a
        // long-lived reader can't monopolise the writer for the pool-wide
        // 30 s. The connection is checked out (writer pool is size 1) so no
        // other writer contends while we override the timeout, and we
        // restore the previous value before returning it to the pool.
        let mut conn = self.pools.write.acquire().await?;
        let prev_busy_ms: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
            .fetch_one(&mut *conn)
            .await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA busy_timeout = {CHECKPOINT_BUSY_TIMEOUT_MS}"
        )))
        .execute(&mut *conn)
        .await?;
        let result = sqlx::query_as::<_, (i64, i64, i64)>("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(&mut *conn)
            .await;
        // Restore the pool-wide busy timeout before this connection is
        // reused for normal writes — best-effort, and always attempted even
        // when the checkpoint itself errored.
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA busy_timeout = {prev_busy_ms}"
        )))
        .execute(&mut *conn)
        .await;
        let (busy, wal_frames, checkpointed_frames) = result?;
        Ok(WalCheckpointOutcome {
            busy: busy != 0,
            wal_frames,
            checkpointed_frames,
        })
    }

    async fn optimize(&self) -> anyhow::Result<()> {
        // Bound the work first (see `ANALYSIS_LIMIT`), then let
        // `PRAGMA optimize` decide which tables actually need `ANALYZE`.
        // Both statements must run on the *same* connection because
        // `analysis_limit` is a per-connection setting — acquire one
        // explicitly rather than issuing two independent pool acquisitions
        // that could land on different connections. Runs on the writer
        // since `optimize` may write `sqlite_stat1`.
        let mut conn = self.pools.write.acquire().await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "PRAGMA analysis_limit = {ANALYSIS_LIMIT}"
        )))
        .execute(&mut *conn)
        .await?;
        sqlx::query("PRAGMA optimize").execute(&mut *conn).await?;
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
