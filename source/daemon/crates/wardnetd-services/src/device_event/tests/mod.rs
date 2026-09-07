mod listener;
mod service;

use std::sync::Arc;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::device::DeviceConnectionMode;
use wardnetd_data::repository::sqlite::{SqliteDeviceEventRepository, SqliteDeviceRepository};
use wardnetd_data::repository::{DeviceRepository, DeviceRow};

use crate::auth_context;
use crate::device_event::service::{DeviceEventService, DeviceEventServiceImpl};

pub const MAC: &str = "8c:86:dd:3d:0f:96";
pub const IP: &str = "192.168.100.23";

pub async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

pub async fn as_admin<T>(fut: impl std::future::Future<Output = T>) -> T {
    auth_context::with_context(AuthContext::system(), fut).await
}

/// A service backed by real SQLite repositories, plus one seeded device.
pub async fn service_with_device(device_id: Uuid) -> Arc<dyn DeviceEventService> {
    let pool = test_pool().await;
    // `devices.zone_id` is a real foreign key; the migrations seed the default
    // zones, so use one of those rather than inventing an id.
    let zone_id: String = sqlx::query_scalar("SELECT id FROM network_zones LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let devices = Arc::new(SqliteDeviceRepository::new(pool.clone()));
    devices
        .insert(&DeviceRow {
            id: device_id.to_string(),
            mac: MAC.to_owned(),
            hostname: None,
            manufacturer: None,
            manufacturer_source: None,
            is_randomized: false,
            device_type: "unknown".to_owned(),
            first_seen: "2026-09-06T00:00:00Z".to_owned(),
            last_seen: "2026-09-06T00:00:00Z".to_owned(),
            last_ip: IP.to_owned(),
            zone_id,
            connection_mode: DeviceConnectionMode::Lan,
        })
        .await
        .unwrap();

    Arc::new(DeviceEventServiceImpl::new(
        Arc::new(SqliteDeviceEventRepository::new(pool.clone())),
        devices,
    ))
}
