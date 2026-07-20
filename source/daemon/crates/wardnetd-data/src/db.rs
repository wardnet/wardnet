use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{
    SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use uuid::Uuid;

/// Reader pool size — bounded so a runaway read can't exhaust file descriptors.
const READ_MAX_CONNECTIONS: u32 = 5;
/// Writer pool is always a single connection — `SQLite` only allows one writer
/// at a time, and serialising at the pool level produces async-aware
/// backpressure instead of busy-spin waits inside `SQLite`.
const WRITE_MAX_CONNECTIONS: u32 = 1;
/// Wait this long for the write lock before returning `SQLITE_BUSY`. The
/// sqlx default of 5 s is shorter than a worst-case `stats_intraday`
/// rollup; bump it so legitimate concurrent writers queue instead of
/// failing.
const BUSY_TIMEOUT: Duration = Duration::from_secs(30);
/// `freelist_count` threshold (in pages) at which the one-shot startup
/// VACUUM kicks in for a legacy `auto_vacuum=NONE` database. ~25k pages
/// at the default 4 KiB page size is ~100 MiB of reclaimable space.
const VACUUM_FREELIST_THRESHOLD_PAGES: i64 = 25_000;
/// Cap the on-disk `-wal` sidecar so a burst of writes (e.g. the daily
/// DNS query-log retention `DELETE`) can't leave the WAL parked at a
/// multi-hundred-MiB high-water mark forever. `SQLite`'s automatic
/// checkpoints are always `PASSIVE` — they let WAL space be *reused* but
/// never *shrink the file*; without this limit the WAL only grows to its
/// historical peak and stays there, slowing every subsequent operation.
/// With the limit set, the checkpointer truncates the file back down to
/// this size once its frames have been folded into the main database.
/// 64 MiB is comfortably above the largest single transaction yet small
/// enough for the Raspberry-Pi-class targets this runs on. The daily
/// [`MaintenanceRepository::wal_checkpoint_truncate`] resets it the rest
/// of the way to ~0.
const WAL_JOURNAL_SIZE_LIMIT_BYTES: i64 = 64 * 1024 * 1024;
const MEMORY_CONNECTION_STRING: &str = ":memory:";
/// File-name suffix for the sentinel a backup restore drops next to the
/// database. Its presence on startup means the `<db>` file was just
/// replaced by a restored snapshot and any leftover WAL/SHM sidecars
/// belong to the *pre-restore* database — they must be discarded, not
/// recovered onto the snapshot.
const RESTORE_PENDING_SUFFIX: &str = ".restore-pending";

/// Path of the restore-pending sentinel for a given database file.
///
/// The backup service writes this after laying down a restored snapshot;
/// [`init_db_pools_from_connection_string`] consumes it on the next
/// startup. Shared here so writer and reader agree on the location.
#[must_use]
pub fn restore_pending_marker_path(db_path: &Path) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(RESTORE_PENDING_SUFFIX);
    PathBuf::from(name)
}

/// Build the path of a sibling sidecar (`-wal`, `-shm`, …) for a database file.
fn sidecar_path(db_path: &Path, suffix: &str) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// If a backup restore is pending, discard the stale WAL/SHM sidecars
/// before any pool opens the database.
///
/// A file-swap restore replaces `<db>` with a complete, self-contained
/// `VACUUM INTO` snapshot, but the *live* WAL-mode pool keeps writing the
/// pre-restore database's frames into `<db>-wal` until the process exits
/// (the restart-pending write, the graceful-shutdown record, …). On the
/// next boot `SQLite` would recover that WAL onto the freshly restored
/// snapshot, silently resurrecting pre-restore state. Gated on the
/// sentinel so a normal crash still recovers its WAL as usual.
pub(crate) fn discard_stale_wal_for_restore(db_path: &Path) {
    let marker = restore_pending_marker_path(db_path);
    if !marker.exists() {
        return;
    }
    for suffix in ["-wal", "-shm"] {
        let sidecar = sidecar_path(db_path, suffix);
        match std::fs::remove_file(&sidecar) {
            Ok(()) => tracing::warn!(
                path = %sidecar.display(),
                "restore pending: discarded stale SQLite sidecar so it isn't recovered onto the restored snapshot",
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::error!(
                path = %sidecar.display(),
                error = %e,
                "restore pending: failed to discard stale SQLite sidecar",
            ),
        }
    }
    if let Err(e) = std::fs::remove_file(&marker)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::error!(
            path = %marker.display(),
            error = %e,
            "restore pending: failed to remove restore sentinel; WAL discard may repeat next boot",
        );
    }
}

/// A reader/writer pair backed by `SQLite`.
///
/// The reader pool runs N connections so concurrent `SELECT`s don't
/// serialise. The writer pool runs a **single** connection so the
/// "one writer at a time" rule `SQLite` enforces internally is expressed
/// at the connection layer — when a writer is busy, a second writer
/// `await`s the connection cooperatively, rather than issuing SQL and
/// busy-spinning against `SQLite`'s internal lock until `busy_timeout`.
///
/// `read` and `write` always point at the same underlying database
/// (same file, same shared-cache URI). They differ only in size.
#[derive(Clone)]
pub struct DbPools {
    /// Multi-connection pool for `SELECT` traffic.
    pub read: SqlitePool,
    /// Single-connection pool for `INSERT`/`UPDATE`/`DELETE` and
    /// transactions. Also the pool that owns migrations.
    pub write: SqlitePool,
}

impl DbPools {
    /// Wrap a single pool as both reader and writer. Used by tests and
    /// the in-memory shared-cache path where splitting would create two
    /// unrelated in-memory databases.
    #[must_use]
    pub fn single(pool: SqlitePool) -> Self {
        Self {
            read: pool.clone(),
            write: pool,
        }
    }
}

/// Initialise the `SQLite` connection pools from a connection string.
///
/// Accepts either the literal string `":memory:"` (for an ephemeral
/// in-memory database — primarily used by the mock server and tests)
/// or a file-system path. `:memory:` is rewritten to a shared in-memory
/// URI so the standard multi-connection pool sees the same database
/// across connections; in that mode the reader and writer collapse to
/// a single pool. File paths get a WAL-mode on-disk database with
/// `auto_vacuum=INCREMENTAL` so freed pages can be reclaimed via
/// `PRAGMA incremental_vacuum`. Migrations run on the writer once both
/// pools are ready.
pub async fn init_db_pools_from_connection_string(conn: &str) -> anyhow::Result<DbPools> {
    let pools = if conn == MEMORY_CONNECTION_STRING {
        // Each call gets a unique shared-memory DB name so one set of
        // pools' connections see each other, but different sets (e.g.
        // parallel tests in the same process) don't collide on schema
        // locks.
        let uri = format!(
            "file:wnm_{}?mode=memory&cache=shared",
            Uuid::new_v4().simple()
        );
        let connect_options = SqliteConnectOptions::new()
            .filename(&uri)
            .journal_mode(SqliteJournalMode::Memory)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(BUSY_TIMEOUT)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(READ_MAX_CONNECTIONS)
            // Keep at least one connection alive for the process lifetime so
            // the shared in-memory DB doesn't get dropped when the last
            // connection is closed.
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(connect_options)
            .await?;
        DbPools::single(pool)
    } else {
        let path = Path::new(conn);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Before opening any pool: if a backup restore just laid down a
        // fresh snapshot, drop the pre-restore WAL/SHM sidecars so SQLite
        // recovers the restored database, not the stale WAL.
        discard_stale_wal_for_restore(path);

        // Shared options used by both pools — they must agree on
        // journal mode, synchronous level, FK enforcement and the
        // auto_vacuum mode the file was created with (the option is
        // persisted in the file header on first creation; passing it
        // here is a no-op on existing files).
        let make_options = || {
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Normal)
                .auto_vacuum(SqliteAutoVacuum::Incremental)
                .busy_timeout(BUSY_TIMEOUT)
                .foreign_keys(true)
                // Bound the on-disk WAL so a passive auto-checkpoint truncates
                // the sidecar back down instead of letting it grow unbounded.
                // Set on every connection because `journal_size_limit` is a
                // per-connection setting in SQLite.
                .pragma(
                    "journal_size_limit",
                    WAL_JOURNAL_SIZE_LIMIT_BYTES.to_string(),
                )
        };

        // Writer first — migrations run on it and they must complete
        // before any reader sees a partially-applied schema. The
        // writer pool's single connection is also the one that runs
        // the legacy-VACUUM startup hook.
        let write = SqlitePoolOptions::new()
            .max_connections(WRITE_MAX_CONNECTIONS)
            .min_connections(1)
            .connect_with(make_options())
            .await?;

        let read = SqlitePoolOptions::new()
            .max_connections(READ_MAX_CONNECTIONS)
            .connect_with(make_options())
            .await?;

        DbPools { read, write }
    };

    sqlx::migrate!("./migrations").run(&pools.write).await?;

    // After migrations, give a legacy `auto_vacuum=NONE` file (created
    // before this commit landed) a one-shot rebuild so the
    // incremental-vacuum machinery has something to reclaim. Skips
    // silently for fresh DBs and for in-memory pools.
    if conn != MEMORY_CONNECTION_STRING {
        startup_vacuum_if_needed(&pools.write, Path::new(conn)).await;
    }

    tracing::info!(connection = conn, "database initialized: connection={conn}");
    Ok(pools)
}

/// Back-compat wrapper for callers that expect a single `SqlitePool`.
/// New code should call [`init_db_pools_from_connection_string`].
pub async fn init_pool_from_connection_string(conn: &str) -> anyhow::Result<SqlitePool> {
    let pools = init_db_pools_from_connection_string(conn).await?;
    Ok(pools.write)
}

/// Initialise the `SQLite` connection pool, creating the database file
/// and running migrations if necessary.
///
/// Thin wrapper around [`init_pool_from_connection_string`] that
/// accepts a filesystem path. Retained for backward compatibility with
/// existing callers (tests and internal code paths).
pub async fn init_pool(db_path: &Path) -> anyhow::Result<SqlitePool> {
    let conn = db_path.to_str().ok_or_else(|| {
        anyhow::anyhow!("database path is not valid UTF-8: {}", db_path.display())
    })?;
    init_pool_from_connection_string(conn).await
}

/// Initialise both pools from a filesystem path.
pub async fn init_db_pools(db_path: &Path) -> anyhow::Result<DbPools> {
    let conn = db_path.to_str().ok_or_else(|| {
        anyhow::anyhow!("database path is not valid UTF-8: {}", db_path.display())
    })?;
    init_db_pools_from_connection_string(conn).await
}

/// If the on-disk database was created with `auto_vacuum=NONE` and is
/// now holding a large freelist (deletes without reclaim), rebuild it
/// once with `auto_vacuum=INCREMENTAL`. After this, `PRAGMA
/// incremental_vacuum` can release pages back to the filesystem.
///
/// Guarded on free disk space — `VACUUM` writes a temporary copy
/// roughly the size of the DB file, so we require ~1.2× headroom on
/// the DB's parent directory. On any error or missing precondition we
/// log and skip; a stuck legacy DB is worse than a slightly chatty
/// startup.
async fn startup_vacuum_if_needed(write: &SqlitePool, db_path: &Path) {
    let auto_vacuum: i64 = match sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(write)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "skip legacy VACUUM check: PRAGMA auto_vacuum failed");
            return;
        }
    };
    if auto_vacuum != 0 {
        // Already INCREMENTAL (2) or FULL (1) — nothing to migrate.
        return;
    }

    let freelist: i64 = match sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(write)
        .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "skip legacy VACUUM check: PRAGMA freelist_count failed");
            return;
        }
    };
    if freelist < VACUUM_FREELIST_THRESHOLD_PAGES {
        tracing::debug!(
            freelist,
            threshold = VACUUM_FREELIST_THRESHOLD_PAGES,
            "legacy auto_vacuum=NONE but freelist under threshold; skipping VACUUM"
        );
        return;
    }

    let db_size = std::fs::metadata(db_path).map_or(0, |m| m.len());
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    match available_disk_bytes(parent) {
        Some(free) if free < db_size.saturating_mul(12) / 10 => {
            tracing::warn!(
                db_size,
                free,
                "legacy auto_vacuum=NONE database is large and reclaimable, but free disk \
                 space is < 1.2x DB size; skipping startup VACUUM. Run `sqlite3 {} 'VACUUM;'` \
                 manually after freeing space.",
                db_path.display(),
            );
            return;
        }
        None => {
            tracing::warn!(
                "could not determine free disk space on {}; skipping startup VACUUM",
                parent.display()
            );
            return;
        }
        Some(_) => {}
    }

    tracing::info!(
        db_size,
        freelist,
        "running one-shot VACUUM to migrate database to auto_vacuum=INCREMENTAL"
    );
    if let Err(e) = sqlx::query("PRAGMA auto_vacuum = INCREMENTAL")
        .execute(write)
        .await
    {
        tracing::warn!(error = %e, "failed to set PRAGMA auto_vacuum=INCREMENTAL");
        return;
    }
    let started = std::time::Instant::now();
    match sqlx::query("VACUUM").execute(write).await {
        Ok(_) => {
            let new_size = std::fs::metadata(db_path).map_or(db_size, |m| m.len());
            tracing::info!(
                elapsed_secs = started.elapsed().as_secs_f64(),
                old_size = db_size,
                new_size,
                "startup VACUUM complete"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "startup VACUUM failed; database remains on auto_vacuum=NONE");
        }
    }
}

/// Best-effort wrapper around `statvfs(3)` returning the bytes
/// available to a non-privileged user on the filesystem that contains
/// `path`. `None` on any failure — failures are logged at `debug` with
/// the OS errno so an operator chasing a stuck startup VACUUM can see
/// *why* the disk-space guard skipped, rather than just *that* it did.
fn available_disk_bytes(path: &Path) -> Option<u64> {
    let c_path = match std::ffi::CString::new(path.as_os_str().as_encoded_bytes()) {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(
                error = %e,
                "available_disk_bytes: path contains NUL byte; cannot stat"
            );
            return None;
        }
    };
    // SAFETY: `c_path` is a valid NUL-terminated C string; `statvfs`
    // writes into the zero-initialised buffer and returns 0 on success.
    let stat = unsafe {
        let mut buf: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c_path.as_ptr(), std::ptr::addr_of_mut!(buf)) != 0 {
            let errno = std::io::Error::last_os_error();
            tracing::debug!(
                error = %errno,
                path = %path.display(),
                "available_disk_bytes: statvfs(3) failed"
            );
            return None;
        }
        buf
    };
    // `f_frsize` is the fundamental block size; `f_bavail` is the
    // number of blocks available to non-root.
    // `cast_lossless` / `useless_conversion`: field types are u32 on macOS
    // and u64 on Linux; `as u64` is the portable no-op-safe widening.
    #[allow(clippy::cast_lossless, clippy::useless_conversion)]
    let frsize = stat.f_frsize as u64;
    #[allow(clippy::cast_lossless, clippy::useless_conversion)]
    let bavail = stat.f_bavail as u64;
    frsize.checked_mul(bavail)
}
