//! Round-trip test for [`SqliteDumper`]: populate an on-disk `SQLite`,
//! dump it, restore the bytes into a fresh file, confirm the data
//! survives.

use std::path::PathBuf;

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Executor, SqlitePool};
use uuid::Uuid;

use crate::database_dumper::{DatabaseDumper, SqliteDumper};

fn temp_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("wardnet-dumper-test-{}.db", Uuid::new_v4()))
}

async fn open_writable(path: &PathBuf) -> SqlitePool {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    SqlitePool::connect_with(opts).await.unwrap()
}

#[tokio::test]
async fn dump_and_restore_round_trip() {
    let src_path = temp_db_path();
    let dst_path = temp_db_path();

    // Populate the source DB with a _sqlx_migrations-like table plus a
    // user-data row so we can verify the restored file preserves both.
    let src_pool = open_writable(&src_path).await;
    src_pool
        .execute(
            r"
            CREATE TABLE _sqlx_migrations (
                version INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time INTEGER NOT NULL
            );
            ",
        )
        .await
        .unwrap();
    src_pool
        .execute(
            r"
            INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
            VALUES (42, 'test', 1, X'00', 1);
            ",
        )
        .await
        .unwrap();
    src_pool
        .execute("CREATE TABLE demo (id INTEGER PRIMARY KEY, label TEXT);")
        .await
        .unwrap();
    src_pool
        .execute("INSERT INTO demo (id, label) VALUES (1, 'hello');")
        .await
        .unwrap();

    let dumper = SqliteDumper::new(src_pool.clone(), src_path.clone());
    let bytes = dumper.dump().await.unwrap();
    assert!(!bytes.is_empty());
    src_pool.close().await;

    // Restore into a fresh destination path.
    let dst_dumper = SqliteDumper::new(
        SqlitePool::connect("sqlite::memory:").await.unwrap(),
        dst_path.clone(),
    );
    let version = dst_dumper.restore(&bytes).await.unwrap();
    assert_eq!(version, 42);

    // Re-open and verify the user table survives.
    let restored = open_writable(&dst_path).await;
    let row: (i64, String) = sqlx::query_as("SELECT id, label FROM demo WHERE id = 1")
        .fetch_one(&restored)
        .await
        .unwrap();
    assert_eq!(row, (1, "hello".to_owned()));
    restored.close().await;

    let _ = tokio::fs::remove_file(&src_path).await;
    let _ = tokio::fs::remove_file(&dst_path).await;
}

#[tokio::test]
async fn current_schema_version_reads_max_version() {
    let path = temp_db_path();
    let pool = open_writable(&path).await;
    pool.execute(
        r"
        CREATE TABLE _sqlx_migrations (
            version INTEGER PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time INTEGER NOT NULL
        );
        ",
    )
    .await
    .unwrap();
    for v in [1, 3, 7, 12] {
        pool.execute(sqlx::AssertSqlSafe(format!(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, \
             execution_time) VALUES ({v}, 'm', 1, X'00', 0);"
        )))
        .await
        .unwrap();
    }

    let dumper = SqliteDumper::new(pool.clone(), path.clone());
    let version = dumper.current_schema_version().await.unwrap();
    assert_eq!(version, 12);
    pool.close().await;

    let _ = tokio::fs::remove_file(&path).await;
}

#[tokio::test]
async fn current_schema_version_is_zero_on_empty_migrations_table() {
    let path = temp_db_path();
    let pool = open_writable(&path).await;
    pool.execute(
        r"
        CREATE TABLE _sqlx_migrations (
            version INTEGER PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
            success BOOLEAN NOT NULL,
            checksum BLOB NOT NULL,
            execution_time INTEGER NOT NULL
        );
        ",
    )
    .await
    .unwrap();

    let dumper = SqliteDumper::new(pool.clone(), path.clone());
    let version = dumper.current_schema_version().await.unwrap();
    assert_eq!(version, 0);
    pool.close().await;

    let _ = tokio::fs::remove_file(&path).await;
}

#[tokio::test]
async fn current_schema_version_errors_when_migrations_table_missing() {
    // `_sqlx_migrations` doesn't exist → the query errors.
    let path = temp_db_path();
    let pool = open_writable(&path).await;

    let dumper = SqliteDumper::new(pool.clone(), path.clone());
    let err = dumper.current_schema_version().await.unwrap_err();
    assert!(
        format!("{err:#}").contains("schema version"),
        "expected wrapped schema-version error, got: {err}"
    );
    pool.close().await;

    let _ = tokio::fs::remove_file(&path).await;
}

#[cfg(unix)]
#[tokio::test]
async fn restore_installs_file_with_0600_perms() {
    use std::os::unix::fs::PermissionsExt;

    // Build a small valid source DB.
    let src_path = temp_db_path();
    let src_pool = open_writable(&src_path).await;
    src_pool
        .execute(
            r"
            CREATE TABLE _sqlx_migrations (
                version INTEGER PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time INTEGER NOT NULL
            );
            INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
            VALUES (1, 'm', 1, X'00', 0);
            ",
        )
        .await
        .unwrap();
    let src_bytes = tokio::fs::read(&src_path).await.unwrap();
    src_pool.close().await;

    let dst_path = temp_db_path();
    let dst_dumper = SqliteDumper::new(
        SqlitePool::connect("sqlite::memory:").await.unwrap(),
        dst_path.clone(),
    );
    dst_dumper.restore(&src_bytes).await.unwrap();

    let mode = tokio::fs::metadata(&dst_path)
        .await
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);

    let _ = tokio::fs::remove_file(&src_path).await;
    let _ = tokio::fs::remove_file(&dst_path).await;
}

#[tokio::test]
async fn restore_rejects_non_sqlite_bytes() {
    let dst_path = temp_db_path();
    let dumper = SqliteDumper::new(
        SqlitePool::connect("sqlite::memory:").await.unwrap(),
        dst_path.clone(),
    );

    let err = dumper
        .restore(b"this-is-not-a-sqlite-file-just-plain-text")
        .await
        .unwrap_err();
    // The bytes land on disk, then the reconnect fails to open them as SQLite.
    assert!(
        format!("{err:#}").to_lowercase().contains("reconnect")
            || format!("{err:#}").to_lowercase().contains("schema version"),
        "expected reconnect/schema error, got: {err}"
    );

    let _ = tokio::fs::remove_file(&dst_path).await;
}
