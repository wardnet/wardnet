use std::path::Path;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{
    SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use uuid::Uuid;

const READ_MAX_CONNECTIONS: u32 = 5;
/// Single writer serialises mutations at the pool layer rather than busy-spinning inside `SQLite`.
const WRITE_MAX_CONNECTIONS: u32 = 1;
const BUSY_TIMEOUT: Duration = Duration::from_secs(30);

/// Reader / writer pool pair backed by `SQLite`.
///
/// `read` and `write` point at the same on-disk database but differ in pool
/// size. Callers must use `write` for all mutations and `read` for
/// `SELECT`-only traffic so the single-writer rule is enforced at the
/// connection layer rather than inside `SQLite`'s lock.
#[derive(Clone)]
pub struct DbPools {
    pub read: SqlitePool,
    pub write: SqlitePool,
}

impl DbPools {
    /// Wrap a single pool as both reader and writer.
    ///
    /// Used by the in-memory path (tests) where split pools would point
    /// at different unnamed in-memory databases.
    #[must_use]
    pub fn single(pool: SqlitePool) -> Self {
        Self {
            read: pool.clone(),
            write: pool,
        }
    }
}

/// Initialise connection pools and run pending migrations.
///
/// Accepts `":memory:"` for an ephemeral in-memory database (tests) or a
/// filesystem path. File-backed databases use WAL mode and
/// `auto_vacuum=INCREMENTAL`.
pub async fn init(database_url: &str) -> anyhow::Result<DbPools> {
    let pools = if database_url == ":memory:" {
        // Unique shared-memory URI so parallel test runs don't collide.
        let uri = format!(
            "file:wnb_{}?mode=memory&cache=shared",
            Uuid::new_v4().simple()
        );
        let opts = SqliteConnectOptions::new()
            .filename(&uri)
            .journal_mode(SqliteJournalMode::Memory)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(BUSY_TIMEOUT)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(READ_MAX_CONNECTIONS)
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(opts)
            .await?;
        DbPools::single(pool)
    } else {
        let path = Path::new(database_url);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let make_opts = || {
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Normal)
                .auto_vacuum(SqliteAutoVacuum::Incremental)
                .busy_timeout(BUSY_TIMEOUT)
                .foreign_keys(true)
        };
        let write = SqlitePoolOptions::new()
            .max_connections(WRITE_MAX_CONNECTIONS)
            .min_connections(1)
            .connect_with(make_opts())
            .await?;
        let read = SqlitePoolOptions::new()
            .max_connections(READ_MAX_CONNECTIONS)
            .connect_with(make_opts())
            .await?;
        DbPools { read, write }
    };

    sqlx::migrate!("./migrations").run(&pools.write).await?;
    tracing::info!(database_url, "database initialised");
    Ok(pools)
}
