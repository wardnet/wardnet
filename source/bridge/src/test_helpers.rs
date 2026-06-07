/// Build a pool connected to an isolated per-test `PostgreSQL` database.
///
/// Requires a `PostgreSQL` server reachable at `BRIDGE_TEST_DATABASE_URL`
/// (default: `postgres://postgres:postgres@127.0.0.1:5432`). The value must be a
/// bare server URL **without** a trailing `/database` path — this helper appends
/// its own database name. Start one locally with:
///
/// ```sh
/// docker compose up -d     # from source/bridge/
/// ```
///
/// In CI a `PostgreSQL` service container is started automatically.
pub async fn test_pool() -> sqlx::PgPool {
    let base_url = std::env::var("BRIDGE_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432".to_string());
    // We append `/<db>` below, so the override must be a bare server URL with no
    // database path component (otherwise we'd build `…/mydb/postgres`). Fail
    // fast on a misconfigured local override rather than emitting a cryptic
    // connection error. (The `postgres://` scheme has two leading slashes; a
    // database path is a third `/` after the host[:port].)
    assert!(
        base_url
            .strip_prefix("postgres://")
            .or_else(|| base_url.strip_prefix("postgresql://"))
            .is_some_and(|rest| !rest.contains('/')),
        "BRIDGE_TEST_DATABASE_URL must be a bare server URL without a /database path, got: {base_url}"
    );

    // Connect to the maintenance database to issue CREATE DATABASE.
    let maintenance_pool = sqlx::PgPool::connect(&format!("{base_url}/postgres"))
        .await
        .expect("Postgres unreachable — run `docker compose up -d` from source/bridge/");

    let db_name = format!("t{}", uuid::Uuid::new_v4().simple());
    // `CREATE DATABASE` is DDL: Postgres cannot bind-parameterise the database
    // identifier, so this inline `format!` is the deliberate, test-only
    // exception to the "query strings are `const &str`" SQL convention — do not
    // copy this pattern into production DML. The name is a fresh UUID-derived
    // identifier (not user input); double-quote it so Postgres treats it as a
    // literal identifier.
    sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
        .execute(&maintenance_pool)
        .await
        .expect("CREATE DATABASE");
    drop(maintenance_pool);

    let pool = sqlx::PgPool::connect(&format!("{base_url}/{db_name}"))
        .await
        .expect("connect to test database");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("apply migrations");
    pool
}
