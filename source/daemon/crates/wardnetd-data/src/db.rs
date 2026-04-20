use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use uuid::Uuid;

const MAX_CONNECTIONS: u32 = 5;
const MEMORY_CONNECTION_STRING: &str = ":memory:";

/// Initialize the `SQLite` connection pool from a connection string.
///
/// Accepts either the literal string `":memory:"` (for an ephemeral in-memory
/// database — primarily used by the mock server and tests) or a file-system
/// path. `:memory:` is rewritten to a shared in-memory URI so the standard
/// multi-connection pool sees the same database across connections. File
/// paths get a WAL-mode on-disk database. Migrations run once the pool is
/// ready.
pub async fn init_pool_from_connection_string(conn: &str) -> anyhow::Result<SqlitePool> {
    let pool = if conn == MEMORY_CONNECTION_STRING {
        // Each call gets a unique shared-memory DB name so one pool's
        // connections see each other, but different pools (e.g. parallel
        // tests in the same process) don't collide on schema locks.
        let uri = format!(
            "file:wnm_{}?mode=memory&cache=shared",
            Uuid::new_v4().simple()
        );
        let connect_options = SqliteConnectOptions::new()
            .filename(&uri)
            .journal_mode(SqliteJournalMode::Memory)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);

        SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            // Keep at least one connection alive for the process lifetime so
            // the shared in-memory DB doesn't get dropped when the last
            // connection is closed.
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(connect_options)
            .await?
    } else {
        let path = Path::new(conn);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let connect_options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);

        SqlitePoolOptions::new()
            .max_connections(MAX_CONNECTIONS)
            .connect_with(connect_options)
            .await?
    };

    sqlx::migrate!("./migrations").run(&pool).await?;

    tracing::info!(connection = conn, "database initialized: connection={conn}");
    Ok(pool)
}

/// Initialize the `SQLite` connection pool, creating the database file and
/// running migrations if necessary.
///
/// Thin wrapper around [`init_pool_from_connection_string`] that accepts a
/// filesystem path. Retained for backward compatibility with existing
/// callers (tests and internal code paths).
pub async fn init_pool(db_path: &Path) -> anyhow::Result<SqlitePool> {
    let conn = db_path.to_str().ok_or_else(|| {
        anyhow::anyhow!("database path is not valid UTF-8: {}", db_path.display())
    })?;
    init_pool_from_connection_string(conn).await
}
