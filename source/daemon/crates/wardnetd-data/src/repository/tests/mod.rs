mod access_request;
mod access_request_migration;
mod anomaly;
mod api_key;
mod device;
mod device_identification;
mod dhcp;
mod dns;
mod dns_events;
mod dns_filter;
mod dns_local;
mod inbound_wg;
mod managed_backfill;
mod network_zone;
mod notification;
mod private_dns;
mod push;
mod routing_profile;
mod session;
mod stats;
mod system_config;
mod tunnel;
mod update;
mod user;
mod user_credential;
mod user_enrolment;
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
