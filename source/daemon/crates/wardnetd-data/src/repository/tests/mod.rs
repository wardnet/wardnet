mod admin;
mod api_key;
mod device;
mod dhcp;
mod dns;
mod dns_events;
mod dns_filter;
mod dns_local;
mod session;
mod stats;
mod system_config;
mod tunnel;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

/// Create an in-memory `SQLite` pool with all migrations applied.
async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}
