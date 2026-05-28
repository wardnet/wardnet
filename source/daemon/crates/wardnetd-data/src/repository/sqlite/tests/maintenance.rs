use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode};
use uuid::Uuid;

use crate::repository::MaintenanceRepository;
use crate::repository::sqlite::maintenance::SqliteMaintenanceRepository;

/// Open a temporary file-backed `SQLite` database with
/// `auto_vacuum=INCREMENTAL`. Returns the pool and the path so the
/// caller can delete the files after the test.
async fn make_incremental_pool() -> (SqlitePool, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "wardnet_vacuum_test_{}.db",
        Uuid::new_v4().simple()
    ));
    let opts = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .auto_vacuum(SqliteAutoVacuum::Incremental);
    let pool = sqlx::SqlitePool::connect_with(opts)
        .await
        .expect("open temp db");
    (pool, path)
}

/// After a bulk `DELETE`, `PRAGMA freelist_count` must be non-zero and
/// `incremental_vacuum` must reclaim at least some of those pages.
#[tokio::test]
async fn incremental_vacuum_reduces_freelist() {
    let (pool, path) = make_incremental_pool().await;

    // Create a scratch table and fill it across several pages.
    // 500 rows × ~200 bytes ≈ 100 KiB > default 4 KiB page, so many
    // pages land on the freelist after the DELETE below.
    sqlx::query("CREATE TABLE _vac_scratch (data TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    let payload = "x".repeat(200);
    for _ in 0..500 {
        sqlx::query("INSERT INTO _vac_scratch (data) VALUES (?)")
            .bind(&payload)
            .execute(&pool)
            .await
            .unwrap();
    }

    // Delete all rows — pages go to the freelist.
    sqlx::query("DELETE FROM _vac_scratch")
        .execute(&pool)
        .await
        .unwrap();

    // Checkpoint WAL so freelist pages are reflected in the DB file.
    sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(&pool)
        .await
        .unwrap();

    let freelist_before: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        freelist_before > 0,
        "expected free pages after bulk DELETE, got {freelist_before}"
    );

    let repo = SqliteMaintenanceRepository::new(pool.clone());
    let reclaimed = repo.incremental_vacuum().await.unwrap();
    assert!(
        reclaimed > 0,
        "incremental_vacuum should reclaim pages, got {reclaimed}"
    );

    let freelist_after: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        freelist_after < freelist_before,
        "freelist should shrink after vacuum: before={freelist_before}, after={freelist_after}"
    );

    // Tidy up temp files.
    pool.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}
