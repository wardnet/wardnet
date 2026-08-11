use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode};
use uuid::Uuid;

use crate::repository::sqlite::maintenance::SqliteMaintenanceRepository;
use crate::repository::{MaintenanceRepository, VacuumStop};

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
    let outcome = repo.incremental_vacuum().await.unwrap();
    assert!(
        outcome.reclaimed_pages > 0,
        "incremental_vacuum should reclaim pages, got {}",
        outcome.reclaimed_pages
    );

    let freelist_after: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        freelist_after < freelist_before,
        "freelist should shrink after vacuum: before={freelist_before}, after={freelist_after}"
    );
    // The outcome is what the daily log line is built from, so it has to
    // describe the run the caller just made, not a rounded version of it.
    assert_eq!(outcome.freelist_before, freelist_before);
    assert_eq!(outcome.freelist_after, freelist_after);
    assert_eq!(outcome.stop, VacuumStop::Drained);
    assert!(
        outcome.chunks >= 1,
        "a reclaiming run must report its chunks"
    );

    // Tidy up temp files.
    pool.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

/// A freelist far larger than one `PRAGMA incremental_vacuum` chunk must be
/// drained in a *single* call, and the database file must actually shrink.
///
/// Regression test for the field bug behind a 2 GiB database that would not
/// come back down: one 2,000-page chunk per daily run against a blocklist
/// refresh that frees ~110k pages per import meant the file could only ever
/// ratchet up to its high-water mark. Under `auto_vacuum=INCREMENTAL` nothing
/// else returns pages to the filesystem, so the assertion that matters is that
/// `page_count` drops — not merely that some pages were reclaimed.
#[tokio::test]
async fn incremental_vacuum_drains_freelist_larger_than_one_chunk() {
    let (pool, path) = make_incremental_pool().await;

    // ~4,000 rows × ~4 KiB ≈ 16 MiB, comfortably past the 2,000-page
    // (~8 MiB) chunk the old fixed budget was capped at.
    sqlx::query("CREATE TABLE _vac_big (data TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    let payload = "z".repeat(4_000);
    for _ in 0..4_000 {
        sqlx::query("INSERT INTO _vac_big (data) VALUES (?)")
            .bind(&payload)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("DELETE FROM _vac_big")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(&pool)
        .await
        .unwrap();

    let freelist_before: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(&pool)
        .await
        .unwrap();
    let pages_before: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        freelist_before > 2_000,
        "test needs a freelist past one chunk to be meaningful, got {freelist_before}"
    );

    let repo = SqliteMaintenanceRepository::new(pool.clone());
    let outcome = repo.incremental_vacuum().await.unwrap();
    let reclaimed = outcome.reclaimed_pages;

    // The old fixed budget capped this at exactly 2,000.
    assert!(
        reclaimed > 2_000,
        "one call should drain past a single chunk, got {reclaimed}"
    );
    assert!(
        outcome.chunks > 1,
        "draining past one chunk must be reported as more than one chunk, got {}",
        outcome.chunks
    );

    let freelist_after: i64 = sqlx::query_scalar("PRAGMA freelist_count")
        .fetch_one(&pool)
        .await
        .unwrap();
    let pages_after: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        freelist_after <= 256,
        "freelist should drain to the floor, got {freelist_after}"
    );
    assert!(
        pages_after < pages_before,
        "the file itself must shrink: before={pages_before} pages, after={pages_after}"
    );

    pool.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

/// The drain must not be derailed by the freelist *growing* while it runs.
///
/// This vacuum runs against a live database where query-log retention and
/// stats cleanup are deleting rows concurrently, so the freelist can be larger
/// after a chunk than before it even though the chunk reclaimed its full
/// 2,000 pages. Terminating on "the freelist didn't shrink" would abort after
/// one chunk exactly when the database is busiest — reinstating the
/// single-chunk-per-day behaviour the chunked loop exists to remove, and
/// reporting 0 reclaimed while doing it. Progress is therefore measured by
/// `page_count`, which only moves when pages are genuinely handed back.
#[tokio::test]
async fn incremental_vacuum_makes_progress_while_the_freelist_grows() {
    let (pool, path) = make_incremental_pool().await;

    sqlx::query("CREATE TABLE _vac_a (data TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE _vac_b (data TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    let payload = "q".repeat(4_000);
    for _ in 0..3_000 {
        sqlx::query("INSERT INTO _vac_a (data) VALUES (?)")
            .bind(&payload)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO _vac_b (data) VALUES (?)")
            .bind(&payload)
            .execute(&pool)
            .await
            .unwrap();
    }
    // Free a large batch up front, leaving `_vac_b` to be freed *during* the
    // vacuum by the concurrent task below.
    sqlx::query("DELETE FROM _vac_a")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(&pool)
        .await
        .unwrap();

    let pages_before: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&pool)
        .await
        .unwrap();

    // Concurrently push more pages onto the freelist while the vacuum runs.
    let churn_pool = pool.clone();
    let churn = tokio::spawn(async move {
        let _ = sqlx::query("DELETE FROM _vac_b").execute(&churn_pool).await;
    });

    let repo = SqliteMaintenanceRepository::new(pool.clone());
    let reclaimed = repo.incremental_vacuum().await.unwrap().reclaimed_pages;
    let _ = churn.await;

    let pages_after: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert!(
        reclaimed > 2_000,
        "concurrent freeing must not cap the drain at one chunk, got {reclaimed}"
    );
    // The reported figure is the real one, not a freelist-delta artefact that
    // concurrent deletes can drive to zero or negative.
    assert_eq!(
        i64::try_from(reclaimed).unwrap(),
        pages_before - pages_after,
        "reclaimed must equal the actual reduction in page_count"
    );

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

// ── last / record maintenance day ────────────────────────────────────────────

/// Minimal `system_config` table — the migrations aren't run for these
/// file-backed pools, and this is the only table the day marker touches.
async fn make_config_table(pool: &SqlitePool) {
    sqlx::query("CREATE TABLE system_config (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
        .execute(pool)
        .await
        .unwrap();
}

/// A database that has never run maintenance reports `None`, which the runner
/// reads as "due now" rather than "already done today".
#[tokio::test]
async fn last_maintenance_day_is_none_before_any_run() {
    let (pool, path) = make_incremental_pool().await;
    make_config_table(&pool).await;
    let repo = SqliteMaintenanceRepository::new(pool.clone());

    assert_eq!(repo.last_maintenance_day().await.unwrap(), None);

    pool.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

/// The day survives the round trip, and re-recording overwrites rather than
/// accumulating rows — the schedule is one marker, not a history.
#[tokio::test]
async fn record_maintenance_day_round_trips_and_overwrites() {
    let (pool, path) = make_incremental_pool().await;
    make_config_table(&pool).await;
    let repo = SqliteMaintenanceRepository::new(pool.clone());

    let first = chrono::NaiveDate::from_ymd_opt(2026, 8, 10).unwrap();
    repo.record_maintenance_day(first).await.unwrap();
    assert_eq!(repo.last_maintenance_day().await.unwrap(), Some(first));

    let second = chrono::NaiveDate::from_ymd_opt(2026, 8, 11).unwrap();
    repo.record_maintenance_day(second).await.unwrap();
    assert_eq!(repo.last_maintenance_day().await.unwrap(), Some(second));

    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM system_config WHERE key = 'db_maintenance_last_run_day'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, 1, "the marker must be a single upserted row");

    pool.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

/// A value that isn't a date reads as "never ran". Raising instead would
/// strand the schedule on one corrupt row forever; running an extra sequence
/// costs nothing and rewrites the value.
#[tokio::test]
async fn last_maintenance_day_treats_an_unparseable_value_as_never_run() {
    let (pool, path) = make_incremental_pool().await;
    make_config_table(&pool).await;
    sqlx::query("INSERT INTO system_config (key, value) VALUES (?, ?)")
        .bind("db_maintenance_last_run_day")
        .bind("not-a-date")
        .execute(&pool)
        .await
        .unwrap();
    let repo = SqliteMaintenanceRepository::new(pool.clone());

    assert_eq!(repo.last_maintenance_day().await.unwrap(), None);

    pool.close().await;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

/// The `stop` tokens are a log contract, not a debug rendering: they are what
/// an operator greps for and what a Loki query matches on, so they must stay
/// stable and lowercase. `budget_exhausted` in particular has no cheap
/// integration test — reaching it needs a freelist past 400,000 pages — so
/// pinning the token here is what keeps it from drifting unnoticed.
#[test]
fn vacuum_stop_tokens_are_stable() {
    assert_eq!(VacuumStop::Drained.as_str(), "drained");
    assert_eq!(VacuumStop::Stalled.as_str(), "stalled");
    assert_eq!(VacuumStop::BudgetExhausted.as_str(), "budget_exhausted");
    // Display and `as_str` must agree — the log line uses one in the
    // structured field and the other in the message text.
    for stop in [
        VacuumStop::Drained,
        VacuumStop::Stalled,
        VacuumStop::BudgetExhausted,
    ] {
        assert_eq!(stop.to_string(), stop.as_str());
    }
}
