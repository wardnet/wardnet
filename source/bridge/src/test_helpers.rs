/// Shared `MySQL` test infrastructure for unit tests.
///
/// Starts a `MySQL` container once per test binary via Docker (testcontainers)
/// and caches the mapped port. Each caller creates its own isolated database
/// schema by calling [`test_pool`].
use tokio::sync::OnceCell;

static MYSQL_PORT: OnceCell<u16> = OnceCell::const_new();

/// Return the host port of the shared `MySQL` test container.
///
/// The container is started on the first call; subsequent calls return the
/// cached port. The container lives for the entire test-binary process.
pub async fn mysql_port() -> u16 {
    *MYSQL_PORT
        .get_or_init(|| async {
            use testcontainers::runners::AsyncRunner;
            use testcontainers_modules::mysql::Mysql;

            let container = Mysql::default()
                .start()
                .await
                .expect("MySQL container failed to start");
            let port = container
                .get_host_port_ipv4(3306)
                .await
                .expect("MySQL mapped port");
            // Leak the container so it stays alive for the entire process.
            Box::leak(Box::new(container));
            port
        })
        .await
}

/// Build a pool connected to an isolated per-test `MySQL` database.
///
/// Each call creates a fresh database named `t{uuid}` on the shared container,
/// runs all migrations, and returns the pool. Tests can run in parallel without
/// interfering with each other.
pub async fn test_pool() -> sqlx::MySqlPool {
    let port = mysql_port().await;
    let root_url = format!("mysql://root:root@127.0.0.1:{port}");

    // Create a unique database for this test invocation.
    let root_pool = sqlx::MySqlPool::connect(&root_url)
        .await
        .expect("connect to MySQL root");
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
        .expect("run migrations");
    pool
}
