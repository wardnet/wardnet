mod admin;
mod api_key;
mod device;
mod dhcp;
mod dns;
mod dns_events;
mod dns_filter;
mod dns_local;
mod inbound_wg;
mod network_zone;
mod notification;
mod push;
mod rule_request;
mod session;
mod stats;
mod system_config;
mod tunnel;
mod zone_exception;

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
