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

/// After writes have grown the `-wal` sidecar, `wal_checkpoint_truncate`
/// truncates it back toward zero.
///
/// This is the exact behaviour the field bug lacked: `SQLite`'s automatic
/// checkpoints are `PASSIVE`, which backfill frames into the main database
/// but leave the `-wal` file parked at its high-water mark on disk. The
/// explicit `TRUNCATE` checkpoint is what shrinks the file, so the
/// assertion that matters is `wal_after < wal_before`.
#[tokio::test]
async fn wal_checkpoint_truncate_shrinks_wal_file() {
    let (pool, path) = make_incremental_pool().await;
    let wal = path.with_extension("db-wal");

    // Generate enough WAL frames to grow the sidecar well past the point a
    // passive auto-checkpoint leaves it (it never truncates the file).
    sqlx::query("CREATE TABLE _wal_scratch (data TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    let payload = "y".repeat(500);
    for _ in 0..2_000 {
        sqlx::query("INSERT INTO _wal_scratch (data) VALUES (?)")
            .bind(&payload)
            .execute(&pool)
            .await
            .unwrap();
    }

    let wal_before = std::fs::metadata(&wal).map_or(0, |m| m.len());
    assert!(
        wal_before > 64 * 1024,
        "expected a non-trivial -wal after inserts, got {wal_before} bytes"
    );

    let repo = SqliteMaintenanceRepository::new(pool.clone());
    let outcome = repo.wal_checkpoint_truncate().await.unwrap();
    assert!(
        !outcome.busy,
        "checkpoint should complete with no competing reader"
    );

    let wal_after = std::fs::metadata(&wal).map_or(0, |m| m.len());
    assert!(
        wal_after < wal_before,
        "TRUNCATE checkpoint should shrink the -wal: before={wal_before}, after={wal_after}"
    );

    pool.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&wal);
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

/// `optimize` runs `PRAGMA analysis_limit` + `PRAGMA optimize` and refreshes
/// planner statistics without error on a live database.
#[tokio::test]
async fn optimize_runs_and_populates_stat_table() {
    let (pool, path) = make_incremental_pool().await;

    // A table with an index gives `PRAGMA optimize` something to analyze.
    sqlx::query("CREATE TABLE _opt_scratch (id INTEGER PRIMARY KEY, k TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE INDEX _opt_scratch_k ON _opt_scratch (k)")
        .execute(&pool)
        .await
        .unwrap();
    for i in 0..500 {
        sqlx::query("INSERT INTO _opt_scratch (k) VALUES (?)")
            .bind(format!("key-{i}"))
            .execute(&pool)
            .await
            .unwrap();
    }

    let repo = SqliteMaintenanceRepository::new(pool.clone());
    repo.optimize().await.expect("optimize should succeed");

    // `PRAGMA optimize` should have run `ANALYZE`, creating `sqlite_stat1`.
    let stat_tables: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name = 'sqlite_stat1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stat_tables, 1,
        "optimize should have created sqlite_stat1 via ANALYZE"
    );

    pool.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

/// `ping` runs `SELECT 1` against the read pool and returns `Ok` while the
/// database is live — the health monitor's `database` probe (issue #214).
#[tokio::test]
async fn ping_succeeds_on_live_database() {
    let (pool, path) = make_incremental_pool().await;
    let repo = SqliteMaintenanceRepository::new(pool.clone());

    repo.ping()
        .await
        .expect("ping a live database should succeed");

    pool.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

/// `ping` surfaces an error once the pool is closed — the path the `database`
/// health check maps to DOWN.
#[tokio::test]
async fn ping_errors_when_pool_closed() {
    let (pool, path) = make_incremental_pool().await;
    let repo = SqliteMaintenanceRepository::new(pool.clone());
    pool.close().await;

    assert!(
        repo.ping().await.is_err(),
        "ping must fail once the pool is closed"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}
