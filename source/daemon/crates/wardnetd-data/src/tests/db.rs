use crate::db::{init_pool, init_pool_from_connection_string};

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
