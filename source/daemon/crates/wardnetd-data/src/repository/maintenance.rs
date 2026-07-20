//! Database-wide maintenance operations that don't belong to any
//! domain repository.
//!
//! Driven daily by the `DbMaintenanceRunner` background task (in the
//! `wardnetd-services` crate): reclaim freed pages
//! ([`incremental_vacuum`](MaintenanceRepository::incremental_vacuum)),
//! shrink the WAL sidecar back to ~0
//! ([`wal_checkpoint_truncate`](MaintenanceRepository::wal_checkpoint_truncate)),
//! and refresh the query planner's statistics
//! ([`optimize`](MaintenanceRepository::optimize)).

use async_trait::async_trait;

/// Result of a `PRAGMA wal_checkpoint(TRUNCATE)`.
///
/// Mirrors the single row `SQLite` returns: `(busy, log, checkpointed)`.
/// A [`busy`](Self::busy) checkpoint means a concurrent reader still held
/// a snapshot of some WAL frames, so the file could **not** be truncated
/// this pass; the next daily tick retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalCheckpointOutcome {
    /// `true` when `SQLite` reported `busy = 1` — a reader blocked the
    /// checkpoint from completing and the WAL was left in place.
    pub busy: bool,
    /// Total number of frames in the WAL at checkpoint time.
    pub wal_frames: i64,
    /// Number of frames successfully moved into the main database.
    pub checkpointed_frames: i64,
}

/// Cross-cutting database maintenance.
#[async_trait]
pub trait MaintenanceRepository: Send + Sync {
    /// Release free pages back to the filesystem in a single bounded
    /// step. No-op on databases created with `auto_vacuum=NONE`.
    ///
    /// The implementation must cap how much work it does per call so
    /// it can't monopolise the writer lock — callers fire this from
    /// background cleanup ticks and expect it to return promptly.
    ///
    /// Returns the number of pages reclaimed, best-effort. A zero
    /// return doesn't imply failure (it can mean the freelist was
    /// already empty, or the file is on `auto_vacuum=NONE`).
    async fn incremental_vacuum(&self) -> anyhow::Result<u64>;

    /// Fold the WAL into the main database and truncate the `-wal`
    /// sidecar back to zero bytes via `PRAGMA wal_checkpoint(TRUNCATE)`.
    ///
    /// `SQLite`'s automatic checkpoints are always `PASSIVE`: they mark WAL
    /// space reusable but never shrink the file on disk, so without a
    /// periodic explicit truncation the WAL parks at its high-water mark
    /// indefinitely. This is that periodic truncation — run daily so the
    /// sidecar returns to ~0 instead of dragging every read and write.
    ///
    /// Runs against the writer connection under a short busy timeout so a
    /// TRUNCATE stuck behind a long-lived reader can't monopolise the
    /// writer. A [`WalCheckpointOutcome::busy`] result is not an error — it
    /// means a reader held a snapshot and the file was left in place for
    /// the next tick to retry.
    async fn wal_checkpoint_truncate(&self) -> anyhow::Result<WalCheckpointOutcome>;

    /// Refresh the query planner's statistics via `PRAGMA optimize`.
    ///
    /// `PRAGMA optimize` runs `ANALYZE` only on the tables whose row
    /// counts have shifted enough since the last run to matter, so it is
    /// cheap to call routinely — unlike a bare `ANALYZE`, which rescans
    /// every table. Bounded by `PRAGMA analysis_limit` so a single index
    /// scan can't monopolise the writer on a large table. Without this the
    /// planner works from stale `sqlite_stat1` data and can pick bad
    /// indexes as tables like `dns_query_log` grow.
    async fn optimize(&self) -> anyhow::Result<()>;

    /// Cheap connectivity probe — runs `SELECT 1` against the read pool and
    /// returns `Ok(())` if the database answered. Used by the health
    /// monitor's `database` check (issue #214); must stay non-blocking and
    /// allocation-free on the hot path.
    async fn ping(&self) -> anyhow::Result<()>;
}
