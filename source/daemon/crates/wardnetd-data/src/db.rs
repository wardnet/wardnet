use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

const MAX_CONNECTIONS: u32 = 5;
const MEMORY_CONNECTION_STRING: &str = ":memory:";

/// Initialize the `SQLite` connection pool from a connection string.
///
/// Accepts either the literal string `":memory:"` (for an ephemeral in-memory
/// database — primarily used by the mock server and tests) or a file-system
/// path. For file paths, a WAL-mode on-disk database is created with the
/// standard multi-connection pool. For `:memory:`, a single-connection pool
/// is used so that migrations and subsequent queries all hit the same
/// in-memory database (each fresh `:memory:` connection otherwise sees its
/// own empty DB), and the journal mode is set to `MEMORY` since WAL is
/// incompatible with `:memory:`.
///
/// Migrations are applied once the pool is ready.
pub async fn init_pool_from_connection_string(conn: &str) -> anyhow::Result<SqlitePool> {
    let pool = if conn == MEMORY_CONNECTION_STRING {
        let connect_options = SqliteConnectOptions::new()
            .in_memory(true)
            .journal_mode(SqliteJournalMode::Memory)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);

        SqlitePoolOptions::new()
            // A single connection keeps all queries on the same in-memory DB.
            .max_connections(1)
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
