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
