//! SQLite-backed [`MaintenanceRepository`] implementation.

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::maintenance::{MaintenanceRepository, WalCheckpointOutcome};

/// How many pages a single `PRAGMA incremental_vacuum(N)` call is
/// allowed to reclaim. Tuned for a Raspberry Pi: ~8 MiB at the `SQLite`
/// default 4 KiB page size — small enough to release the writer lock
/// promptly so concurrent flushes don't queue against `busy_timeout`.
const INCREMENTAL_VACUUM_CHUNK_PAGES: u32 = 2_000;

/// Upper bound on chunks per daily run — ~1.6 GiB at the 4 KiB page size.
///
/// One chunk per daily tick used to be the whole budget, which is what let a
/// 2 GiB field database sit at 788 MiB of freelist that could never come back:
/// a blocklist refresh frees ~110k pages when it reclaims a superseded
/// generation, against 2k reclaimed per day. Under `auto_vacuum=INCREMENTAL`
/// the file only ever shrinks here, so the budget has to track the freelist
/// rather than a constant — otherwise the file ratchets to its high-water mark
/// and stays there. The ceiling still bounds a single run.
const INCREMENTAL_VACUUM_MAX_CHUNKS: u32 = 200;

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
        // `PRAGMA incremental_vacuum(N)` is a no-op on databases created with
        // `auto_vacuum=NONE`, so it is safe to call unconditionally.
        //
        // Progress is measured by `page_count`, not `freelist_count`. The
        // freelist is the wrong yardstick here: this runs against a live
        // database where query-log retention and stats cleanup are freeing
        // pages concurrently, so the freelist can grow across a chunk even
        // though the chunk reclaimed its full 2,000 pages. Terminating on
        // "freelist didn't shrink" would abort after one chunk exactly when
        // the database is busiest — reinstating the single-chunk-per-day
        // behaviour this loop exists to remove, and reporting 0 reclaimed
        // while doing it. `page_count` only moves when pages are actually
        // returned to the filesystem, which is the thing being asked for.
        let stmt = format!("PRAGMA incremental_vacuum({INCREMENTAL_VACUUM_CHUNK_PAGES})");
        let mut reclaimed: i64 = 0;

        // Chunked rather than one big call: each `PRAGMA` takes and releases
        // the writer, so the DNS query log and stats flushes interleave instead
        // of queueing behind a multi-hundred-megabyte reclaim.
        // Drains to empty rather than stopping at a floor: a small deployment
        // whose freelist never reaches one chunk still has to get its pages
        // back, and `PRAGMA incremental_vacuum(N)` already reclaims only
        // `min(N, freelist)` so a partial final chunk costs nothing.
        for _ in 0..INCREMENTAL_VACUUM_MAX_CHUNKS {
            // Errors propagate rather than defaulting: a read that fails
            // because the writer is busy must not be mistaken for "freelist is
            // empty, nothing to do", which would silently skip the whole daily
            // reclaim. The caller logs it and the next tick retries.
            let free: i64 = sqlx::query_scalar("PRAGMA freelist_count")
                .fetch_one(&self.pools.write)
                .await?;
            if free <= 0 {
                break;
            }
            let pages_before: i64 = sqlx::query_scalar("PRAGMA page_count")
                .fetch_one(&self.pools.write)
                .await?;

            sqlx::query(sqlx::AssertSqlSafe(stmt.clone()))
                .execute(&self.pools.write)
                .await?;

            let pages_after: i64 = sqlx::query_scalar("PRAGMA page_count")
                .fetch_one(&self.pools.write)
                .await?;
            let freed = pages_before - pages_after;
            if freed <= 0 {
                // Nothing came back this pass: `auto_vacuum=NONE`, or every
                // remaining free page sits below a pointer-map page that
                // cannot be relocated yet. Stop rather than spinning the
                // writer for the rest of the budget.
                break;
            }
            reclaimed += freed;
            tokio::task::yield_now().await;
        }

        Ok(u64::try_from(reclaimed).unwrap_or(0))
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
