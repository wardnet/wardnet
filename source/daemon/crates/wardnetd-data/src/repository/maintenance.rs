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

/// Why an incremental vacuum stopped reclaiming pages.
///
/// A run that reclaims nothing is the normal case on a healthy database and
/// the symptom of a stuck one, and the page count alone cannot tell them
/// apart. This says which happened, so the daily log line an operator reads is
/// a diagnosis rather than a number to squint at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VacuumStop {
    /// The freelist reached zero — everything that could come back, did.
    Drained,
    /// A chunk returned no pages while the freelist was still non-empty.
    ///
    /// Expected transiently: `auto_vacuum=NONE` (where the pragma is a no-op),
    /// or free pages sitting below a pointer-map page that cannot be relocated
    /// yet. Persisting across days with a large freelist is the signature of a
    /// database that has stopped shrinking, which is the failure this whole
    /// outcome exists to make visible.
    Stalled,
    /// The per-run chunk budget ran out with pages still on the freelist. Not a
    /// fault — the next daily run continues from here.
    BudgetExhausted,
}

impl VacuumStop {
    /// Stable lowercase token for structured logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Drained => "drained",
            Self::Stalled => "stalled",
            Self::BudgetExhausted => "budget_exhausted",
        }
    }
}

impl std::fmt::Display for VacuumStop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Result of one bounded incremental-vacuum run.
///
/// Carries the freelist on both sides of the run rather than just the pages
/// reclaimed. The two answer different questions: `reclaimed_pages` says how
/// much came back, `freelist_after` says how much is still owed, and only the
/// pair distinguishes "nothing to do" from "could not do anything".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncrementalVacuumOutcome {
    /// Pages returned to the filesystem, measured as the drop in
    /// `PRAGMA page_count` across the run.
    pub reclaimed_pages: u64,
    /// `PRAGMA freelist_count` before the first chunk.
    pub freelist_before: i64,
    /// `PRAGMA freelist_count` after the last chunk. Can exceed
    /// `freelist_before` on a busy database — retention deletes free pages
    /// concurrently — which is why it is reported rather than differenced.
    pub freelist_after: i64,
    /// `PRAGMA page_count` after the last chunk: the current size of the
    /// database file, in pages.
    pub page_count_after: i64,
    /// How many chunks ran. Zero means the freelist was empty on entry.
    pub chunks: u32,
    /// Why the loop ended.
    pub stop: VacuumStop,
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
    /// Returns an [`IncrementalVacuumOutcome`] describing the run,
    /// best-effort. Zero reclaimed pages doesn't imply failure — see
    /// [`VacuumStop`] for what separates the benign cases from the one worth
    /// acting on.
    async fn incremental_vacuum(&self) -> anyhow::Result<IncrementalVacuumOutcome>;

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

    /// The UTC calendar date of the last completed daily maintenance
    /// sequence, or `None` if it has never run on this database.
    ///
    /// Persisted rather than held in memory because the schedule is
    /// "once per calendar day", and an in-process marker restarts the clock on
    /// every daemon restart: a box that restarts before its daily rollover
    /// never reaches one. That is not hypothetical — the daemon restarts for
    /// its own updates, and the field database this exists to shrink had
    /// accumulated 411 MB of unreclaimed freelist.
    async fn last_maintenance_day(&self) -> anyhow::Result<Option<chrono::NaiveDate>>;

    /// Record `day` as the date of the last completed daily maintenance
    /// sequence. See [`last_maintenance_day`](Self::last_maintenance_day).
    async fn record_maintenance_day(&self, day: chrono::NaiveDate) -> anyhow::Result<()>;

    /// Cheap connectivity probe — runs `SELECT 1` against the read pool and
    /// returns `Ok(())` if the database answered. Used by the health
    /// monitor's `database` check (issue #214); must stay non-blocking and
    /// allocation-free on the hot path.
    async fn ping(&self) -> anyhow::Result<()>;
}
