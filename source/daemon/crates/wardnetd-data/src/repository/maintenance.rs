//! Database-wide maintenance operations that don't belong to any
//! domain repository.
//!
//! Today there's exactly one — [`incremental_vacuum`](MaintenanceRepository::incremental_vacuum)
//! — driven by the DNS query-log cleanup runner after a bulk DELETE.
//! Future operations of the same shape (e.g. `PRAGMA wal_checkpoint`,
//! `ANALYZE`) belong here.

use async_trait::async_trait;

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

    /// Cheap connectivity probe — runs `SELECT 1` against the read pool and
    /// returns `Ok(())` if the database answered. Used by the health
    /// monitor's `database` check (issue #214); must stay non-blocking and
    /// allocation-free on the hot path.
    async fn ping(&self) -> anyhow::Result<()>;
}
