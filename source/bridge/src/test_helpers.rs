/// Build a pool connected to an isolated per-test `MySQL` database.
///
/// Requires a `MySQL` server reachable at `BRIDGE_TEST_DATABASE_URL`
/// (default: `mysql://root:root@127.0.0.1:3306`). Start one locally with:
///
/// ```sh
/// docker compose up -d     # from source/bridge/
/// ```
///
/// In CI a `MySQL` service container is started automatically.
pub async fn test_pool() -> sqlx::MySqlPool {
    let root_url = std::env::var("BRIDGE_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "mysql://root:root@127.0.0.1:3306".to_string());

    let root_pool = sqlx::MySqlPool::connect(&root_url)
        .await
        .expect("MySQL unreachable — run `docker compose up -d` from source/bridge/");

    let db_name = format!("t{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE `{db_name}`"))
        .execute(&root_pool)
        .await
        .expect("CREATE DATABASE");
    drop(root_pool);

    let pool = sqlx::MySqlPool::connect(&format!("{root_url}/{db_name}"))
        .await
        .expect("connect to test database");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("apply migrations");
    pool
}
