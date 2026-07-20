use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use sqlx::{ConnectOptions, Connection};
use uuid::Uuid;

use crate::db::{
    discard_stale_wal_for_restore, init_db_pools_from_connection_string, init_pool,
    init_pool_from_connection_string, restore_pending_marker_path,
};

/// Sibling sidecar path (`-wal` / `-shm`) for a database file.
fn sidecar(db: &Path, suffix: &str) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Open `path` (WAL mode) and read the single `kv.channel` value. Used by
/// the restore-WAL regression test to observe which database state wins.
async fn read_channel(path: &Path) -> String {
    let mut conn = SqliteConnectOptions::new()
        .filename(path)
        .journal_mode(SqliteJournalMode::Wal)
        .connect()
        .await
        .unwrap();
    let v: String = sqlx::query_scalar("SELECT v FROM kv WHERE k = 'channel'")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    conn.close().await.unwrap();
    v
}

#[tokio::test]
async fn init_pool_from_connection_string_in_memory_runs_migrations() {
    let pool = init_pool_from_connection_string(":memory:")
        .await
        .expect("in-memory pool creation should succeed");

    // Migrations should have created at least the `admins` and `devices`
    // tables (both present in the initial schema). A simple `SELECT` verifies
    // the connection is live and the schema applied.
    let table_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table'")
            .fetch_one(&pool)
            .await
            .expect("sqlite_master query should succeed");

    assert!(
        table_count > 0,
        "expected migrations to create tables, got count={table_count}"
    );
}

#[tokio::test]
async fn init_pool_from_connection_string_in_memory_persists_writes() {
    // Regression: with a multi-connection pool, writes on one ":memory:"
    // connection would not be visible on another. The single-connection
    // config must keep reads and writes on the same DB.
    let pool = init_pool_from_connection_string(":memory:")
        .await
        .expect("in-memory pool creation should succeed");

    sqlx::query("INSERT INTO system_config (key, value) VALUES ('mock_test', 'ok')")
        .execute(&pool)
        .await
        .expect("insert should succeed");

    let value: String =
        sqlx::query_scalar("SELECT value FROM system_config WHERE key = 'mock_test'")
            .fetch_one(&pool)
            .await
            .expect("select should return the inserted row");

    assert_eq!(value, "ok");
}

#[tokio::test]
async fn restore_marker_discards_stale_wal_so_snapshot_wins() {
    // Regression for the backup-restore WAL bug: a file-swap restore lays
    // down a fresh snapshot as `<db>`, but the live WAL-mode pool keeps
    // writing the pre-restore database's frames into `<db>-wal` until the
    // process restarts. Without the restore sentinel, SQLite recovers that
    // stale WAL onto the snapshot on the next boot and resurrects
    // pre-restore state. The guard must discard the sidecars so the
    // snapshot wins.
    let dir = std::env::temp_dir();
    let live = dir.join(format!("wardnet-wal-live-{}.db", Uuid::new_v4()));

    // Build a WAL-mode DB, fold a "stable" value fully into the main file
    // (checkpoint TRUNCATE empties the WAL), and capture the main bytes as
    // the restore *snapshot*.
    let mut conn = SqliteConnectOptions::new()
        .filename(&live)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .connect()
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_autocheckpoint=0")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT NOT NULL)")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO kv (k, v) VALUES ('channel', 'stable')")
        .execute(&mut conn)
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut conn)
        .await
        .unwrap();
    let snapshot = tokio::fs::read(&live).await.unwrap();

    // Write a post-snapshot change that lands in the WAL (not checkpointed),
    // and capture the populated -wal while the connection is still open.
    sqlx::query("UPDATE kv SET v = 'beta' WHERE k = 'channel'")
        .execute(&mut conn)
        .await
        .unwrap();
    let wal = tokio::fs::read(sidecar(&live, "-wal")).await.unwrap();
    assert!(
        !wal.is_empty(),
        "expected a populated -wal carrying the post-snapshot change"
    );
    conn.close().await.unwrap();
    let _ = tokio::fs::remove_file(&live).await;
    let _ = tokio::fs::remove_file(sidecar(&live, "-wal")).await;
    let _ = tokio::fs::remove_file(sidecar(&live, "-shm")).await;

    // --- Control: snapshot + stale WAL, no marker → SQLite recovers the
    //     WAL and 'beta' resurfaces. Proves the failure this guard fixes. ---
    let ctrl = dir.join(format!("wardnet-wal-ctrl-{}.db", Uuid::new_v4()));
    tokio::fs::write(&ctrl, &snapshot).await.unwrap();
    tokio::fs::write(sidecar(&ctrl, "-wal"), &wal)
        .await
        .unwrap();
    assert_eq!(
        read_channel(&ctrl).await,
        "beta",
        "control: stale WAL is recovered without the restore marker"
    );

    // --- Fix: same triple plus the marker → the guard discards the
    //     sidecars and the snapshot's 'stable' wins. ---
    let fixed = dir.join(format!("wardnet-wal-fixed-{}.db", Uuid::new_v4()));
    tokio::fs::write(&fixed, &snapshot).await.unwrap();
    tokio::fs::write(sidecar(&fixed, "-wal"), &wal)
        .await
        .unwrap();
    // A stale -shm too, to prove the guard clears it.
    tokio::fs::write(sidecar(&fixed, "-shm"), b"stale")
        .await
        .unwrap();
    tokio::fs::write(restore_pending_marker_path(&fixed), b"")
        .await
        .unwrap();

    discard_stale_wal_for_restore(&fixed);

    assert!(!sidecar(&fixed, "-wal").exists(), "guard must remove -wal");
    assert!(!sidecar(&fixed, "-shm").exists(), "guard must remove -shm");
    assert!(
        !restore_pending_marker_path(&fixed).exists(),
        "guard must consume the restore marker"
    );
    assert_eq!(
        read_channel(&fixed).await,
        "stable",
        "restored snapshot must win after the guard discards the stale WAL"
    );

    for p in [&ctrl, &fixed] {
        let _ = tokio::fs::remove_file(p).await;
        let _ = tokio::fs::remove_file(sidecar(p, "-wal")).await;
        let _ = tokio::fs::remove_file(sidecar(p, "-shm")).await;
    }
}

#[tokio::test]
async fn init_db_pools_with_file_path_creates_migrates_and_runs_the_restore_guard() {
    // Exercises the on-disk (file) branch — including the restore-WAL
    // guard call — rather than the `:memory:` branch the other tests use.
    let dir = std::env::temp_dir();
    let db = dir.join(format!("wardnet-init-file-{}.db", Uuid::new_v4()));

    let pools = init_db_pools_from_connection_string(db.to_str().unwrap())
        .await
        .expect("file-based pool creation should succeed");

    let table_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table'")
            .fetch_one(&pools.read)
            .await
            .expect("sqlite_master query should succeed");
    assert!(table_count > 0, "migrations should have created tables");

    pools.write.close().await;
    pools.read.close().await;
    for s in ["", "-wal", "-shm"] {
        let _ = tokio::fs::remove_file(sidecar(&db, s)).await;
    }
}

#[tokio::test]
async fn file_pools_cap_the_wal_journal_size_limit() {
    // Regression for the 530 MiB WAL bloat: every file-backed connection
    // must open with `journal_size_limit` set so a passive auto-checkpoint
    // truncates the sidecar back down instead of letting it grow unbounded.
    // A limit of 0 (SQLite's default) means "never truncate" — the bug.
    let dir = std::env::temp_dir();
    let db = dir.join(format!("wardnet-wal-limit-{}.db", Uuid::new_v4()));

    let pools = init_db_pools_from_connection_string(db.to_str().unwrap())
        .await
        .expect("file-based pool creation should succeed");

    for pool in [&pools.write, &pools.read] {
        let limit: i64 = sqlx::query_scalar("PRAGMA journal_size_limit")
            .fetch_one(pool)
            .await
            .expect("journal_size_limit query should succeed");
        assert_eq!(
            limit,
            64 * 1024 * 1024,
            "every file connection must cap the WAL at 64 MiB, got {limit}"
        );
    }

    pools.write.close().await;
    pools.read.close().await;
    for s in ["", "-wal", "-shm"] {
        let _ = tokio::fs::remove_file(sidecar(&db, s)).await;
    }
}

#[tokio::test]
async fn discard_stale_wal_tolerates_unremovable_sidecar_and_marker() {
    // The guard must never panic on filesystem errors. Make both the
    // `-wal` sidecar and the marker directories so `remove_file` fails
    // with a non-NotFound error, and leave `-shm` absent so the NotFound
    // arm is exercised too.
    let dir = std::env::temp_dir();
    let db = dir.join(format!("wardnet-wal-err-{}.db", Uuid::new_v4()));
    std::fs::create_dir(restore_pending_marker_path(&db)).unwrap();
    std::fs::create_dir(sidecar(&db, "-wal")).unwrap();

    discard_stale_wal_for_restore(&db);

    // Errors were swallowed; the unremovable entries remain.
    assert!(sidecar(&db, "-wal").exists());
    assert!(restore_pending_marker_path(&db).exists());

    let _ = std::fs::remove_dir_all(sidecar(&db, "-wal"));
    let _ = std::fs::remove_dir_all(restore_pending_marker_path(&db));
}

#[tokio::test]
async fn discard_stale_wal_is_a_noop_without_the_marker() {
    // Without the sentinel a normal crash must keep its WAL so SQLite can
    // recover it — the guard must leave the sidecars untouched.
    let dir = std::env::temp_dir();
    let db = dir.join(format!("wardnet-wal-noop-{}.db", Uuid::new_v4()));
    tokio::fs::write(&db, b"main").await.unwrap();
    tokio::fs::write(sidecar(&db, "-wal"), b"wal")
        .await
        .unwrap();

    discard_stale_wal_for_restore(&db);

    assert!(
        sidecar(&db, "-wal").exists(),
        "no marker → WAL must be preserved for normal crash recovery"
    );

    let _ = tokio::fs::remove_file(&db).await;
    let _ = tokio::fs::remove_file(sidecar(&db, "-wal")).await;
}

#[tokio::test]
async fn init_pool_with_path_delegates_to_connection_string() {
    // The legacy `init_pool(&Path)` entrypoint accepts ":memory:" as a path
    // and routes through the same in-memory code path.
    let pool = init_pool(std::path::Path::new(":memory:"))
        .await
        .expect("path-based in-memory pool should succeed");

    let result: i64 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&pool)
        .await
        .expect("trivial query should succeed");

    assert_eq!(result, 1);
}
