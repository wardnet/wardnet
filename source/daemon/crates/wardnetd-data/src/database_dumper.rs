//! `SQLite` database dumper — captures a consistent snapshot for backup
//! and lays down a restored database in place.
//!
//! We use `VACUUM INTO` rather than an fs-level `cp` because `SQLite` is
//! mid-write during normal daemon operation (WAL mode, background
//! writers): `VACUUM INTO` takes an implicit read lock, flushes the
//! WAL, and writes a fresh, consistent file. The resulting snapshot is
//! safe to pack into the bundle without worrying about half-written
//! pages or torn WAL.
//!
//! On restore the dumper reconnects to the target path, runs every
//! migration shipped with the running daemon, and leaves the pool in a
//! ready-to-use state. Callers are expected to have stopped the
//! database-dependent runners (DHCP/DNS/update) before calling
//! [`DatabaseDumper::restore`].

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Executor, SqlitePool};

/// Captures and restores `SQLite` snapshots for the backup subsystem.
#[async_trait]
pub trait DatabaseDumper: Send + Sync {
    /// Produce a consistent snapshot of the live database as a byte
    /// buffer. Internally writes to a temp file via `VACUUM INTO`, then
    /// reads the file back and deletes it.
    async fn dump(&self) -> anyhow::Result<Vec<u8>>;

    /// Restore `bytes` into the live database path, replacing whatever
    /// is there. The caller is responsible for stopping runners and
    /// closing pools *before* calling; this method takes no lock.
    ///
    /// The returned `i64` is the highest applied migration version in
    /// the restored database — surfaced so the caller can log it and
    /// decide whether to trigger an online migration pass.
    async fn restore(&self, bytes: &[u8]) -> anyhow::Result<i64>;

    /// Read the highest applied migration version from the live
    /// database. Used to build `BundleManifest.schema_version` on
    /// export and to refuse imports from a newer schema than the
    /// running daemon supports.
    async fn current_schema_version(&self) -> anyhow::Result<i64>;
}

/// Default [`DatabaseDumper`] backed by a live [`SqlitePool`] plus a
/// known filesystem path to the database.
///
/// Construction is cheap — the pool is handed in by the caller (same
/// pool the repositories use), and `database_path` is the
/// already-resolved path `SqlitePool` is backed by. We re-derive it
/// from config at wiring time rather than trying to pry it out of the
/// pool, because `SqlitePool` doesn't expose the URL.
pub struct SqliteDumper {
    pool: SqlitePool,
    database_path: PathBuf,
}

impl SqliteDumper {
    /// Create a new dumper. `database_path` must be the on-disk file
    /// backing `pool`; we use it for the `VACUUM INTO` destination
    /// directory and for the restore write target.
    #[must_use]
    pub fn new(pool: SqlitePool, database_path: PathBuf) -> Self {
        Self {
            pool,
            database_path,
        }
    }

    /// Directory we stage temp dumps into. Always a sibling of the live
    /// DB so the `VACUUM INTO` write lands on the same filesystem.
    fn staging_dir(&self) -> PathBuf {
        self.database_path
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    }

    /// Generate a unique temp-file path for a snapshot. Includes a UUID
    /// so concurrent export requests (shouldn't happen, but might) don't
    /// clobber each other.
    fn temp_dump_path(&self) -> PathBuf {
        let id = uuid::Uuid::new_v4();
        self.staging_dir().join(format!(".wardnet-dump-{id}.db"))
    }
}

#[async_trait]
impl DatabaseDumper for SqliteDumper {
    async fn dump(&self) -> anyhow::Result<Vec<u8>> {
        let dump_path = self.temp_dump_path();
        tokio::fs::create_dir_all(self.staging_dir())
            .await
            .map_err(|e| anyhow::anyhow!("failed to create dump staging dir: {e}"))?;

        // `VACUUM INTO` takes a string literal for the destination; we
        // can't parameter-bind it. Sanitise by using a UUID-derived
        // path we just built — there's no operator-influenced input on
        // this line, but we still escape single quotes defensively.
        let dest = dump_path.to_string_lossy().replace('\'', "''");
        let sql = format!("VACUUM INTO '{dest}'");
        self.pool
            .execute(sql.as_str())
            .await
            .map_err(|e| anyhow::anyhow!("VACUUM INTO failed: {e}"))?;

        let bytes = tokio::fs::read(&dump_path).await.map_err(|e| {
            anyhow::anyhow!(
                "failed to read back vacuumed snapshot at {}: {e}",
                dump_path.display()
            )
        })?;

        if let Err(e) = tokio::fs::remove_file(&dump_path).await {
            tracing::warn!(
                path = %dump_path.display(),
                error = %e,
                "failed to clean up temp dump file, continuing: path={path}, error={e}",
                path = dump_path.display(),
            );
        }

        tracing::info!(
            bytes = bytes.len(),
            "database snapshot captured: bytes={bytes}",
            bytes = bytes.len(),
        );
        Ok(bytes)
    }

    async fn restore(&self, bytes: &[u8]) -> anyhow::Result<i64> {
        // Atomic-ish: write to a sibling temp file, rename over the live
        // path. Rename on the same filesystem is atomic on Linux, which
        // guarantees the daemon never observes a half-written DB.
        let staging = self
            .database_path
            .with_file_name(format!(".wardnet-restore-{}.db", uuid::Uuid::new_v4()));

        tokio::fs::write(&staging, bytes).await.map_err(|e| {
            anyhow::anyhow!(
                "failed to write restored database to {}: {e}",
                staging.display()
            )
        })?;

        tokio::fs::rename(&staging, &self.database_path)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "failed to swap restored database into {}: {e}",
                    self.database_path.display()
                )
            })?;

        // Reconnect against the fresh file and read back the schema
        // version — lets the caller log it and decide whether to run
        // migrations. We open a throwaway pool here because the live
        // pool is still pointed at the (deleted) old file; the real
        // pool is rebuilt by the service after this method returns.
        let opts = SqliteConnectOptions::new()
            .filename(&self.database_path)
            .create_if_missing(false)
            .read_only(true);
        let pool = SqlitePool::connect_with(opts).await.map_err(|e| {
            anyhow::anyhow!(
                "failed to reconnect to restored database at {}: {e}",
                self.database_path.display()
            )
        })?;

        let row: (i64,) = sqlx::query_as("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read schema version from restored db: {e}"))?;

        pool.close().await;

        tracing::info!(
            schema_version = row.0,
            "database restore complete: schema_version={schema_version}",
            schema_version = row.0,
        );
        Ok(row.0)
    }

    async fn current_schema_version(&self) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("failed to read current schema version: {e}"))?;
        Ok(row.0)
    }
}
