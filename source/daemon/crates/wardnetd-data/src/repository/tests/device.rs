use super::test_pool;
use crate::repository::device::DeviceRow;
use crate::repository::{DeviceRepository, SqliteDeviceRepository};
use wardnet_common::device::DeviceType;
use wardnet_common::routing::{RoutingTarget, RuleCreator};

const DEV1: &str = "00000000-0000-0000-0000-000000000001";
const DEV2: &str = "00000000-0000-0000-0000-000000000002";
const DEV3: &str = "00000000-0000-0000-0000-000000000003";

fn sample_device_row(id: &str, mac: &str, ip: &str) -> DeviceRow {
    DeviceRow {
        id: id.to_owned(),
        mac: mac.to_owned(),
        hostname: Some("my-host".to_owned()),
        manufacturer: Some("Apple".to_owned()),
        device_type: "phone".to_owned(),
        first_seen: "2026-03-07T00:00:00Z".to_owned(),
        last_seen: "2026-03-07T00:00:00Z".to_owned(),
        last_ip: ip.to_owned(),
    }
}

async fn insert_device(pool: &sqlx::SqlitePool, id: &str, mac: &str, ip: &str) {
    let now = "2026-03-07T00:00:00Z";
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen) \
         VALUES (?, ?, ?, 'unknown', ?, ?)",
    )
    .bind(id)
    .bind(mac)
    .bind(ip)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn find_by_ip_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let device = repo.find_by_ip("192.168.1.10").await.unwrap().unwrap();
    assert_eq!(device.id.to_string(), DEV1);
    assert_eq!(device.mac, "AA:BB:CC:DD:EE:01");
    assert_eq!(device.last_ip, "192.168.1.10");
    assert_eq!(device.device_type, DeviceType::Unknown);
    assert!(!device.admin_locked);
}

#[tokio::test]
async fn find_by_ip_not_found() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let result = repo.find_by_ip("10.0.0.99").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn find_by_id_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.mac, "AA:BB:CC:DD:EE:01");
}

#[tokio::test]
async fn find_by_mac_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let device = repo
        .find_by_mac("AA:BB:CC:DD:EE:01")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(device.id.to_string(), DEV1);
}

#[tokio::test]
async fn find_by_mac_not_found() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let result = repo.find_by_mac("FF:FF:FF:FF:FF:FF").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn insert_new_device() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let row = sample_device_row(DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10");
    repo.insert(&row).await.unwrap();

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.mac, "AA:BB:CC:DD:EE:01");
    assert_eq!(device.hostname, Some("my-host".to_owned()));
    assert_eq!(device.manufacturer, Some("Apple".to_owned()));
    assert_eq!(device.device_type, DeviceType::Phone);
    assert_eq!(device.last_ip, "192.168.1.10");
}

#[tokio::test]
async fn update_last_seen_and_ip() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    let row = sample_device_row(DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10");
    repo.insert(&row).await.unwrap();

    repo.update_last_seen_and_ip(DEV1, "192.168.1.20", "2026-03-07T12:00:00Z")
        .await
        .unwrap();

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.last_ip, "192.168.1.20");
    assert_eq!(device.last_seen.to_rfc3339(), "2026-03-07T12:00:00+00:00");
}

#[tokio::test]
async fn update_last_seen_batch_multiple() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.insert(&sample_device_row(
        DEV1,
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();
    repo.insert(&sample_device_row(
        DEV2,
        "AA:BB:CC:DD:EE:02",
        "192.168.1.11",
    ))
    .await
    .unwrap();

    let updates = vec![
        (DEV1.to_owned(), "2026-03-07T06:00:00Z".to_owned()),
        (DEV2.to_owned(), "2026-03-07T07:00:00Z".to_owned()),
    ];
    repo.update_last_seen_batch(&updates).await.unwrap();

    let d1 = repo.find_by_id(DEV1).await.unwrap().unwrap();
    let d2 = repo.find_by_id(DEV2).await.unwrap().unwrap();
    assert_eq!(d1.last_seen.to_rfc3339(), "2026-03-07T06:00:00+00:00");
    assert_eq!(d2.last_seen.to_rfc3339(), "2026-03-07T07:00:00+00:00");
}

#[tokio::test]
async fn update_hostname() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.insert(&sample_device_row(
        DEV1,
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();

    repo.update_hostname(DEV1, "new-hostname").await.unwrap();

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.hostname, Some("new-hostname".to_owned()));
}

#[tokio::test]
async fn update_name_and_type() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.insert(&sample_device_row(
        DEV1,
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();

    repo.update_name_and_type(DEV1, Some("Living Room TV"), "tv")
        .await
        .unwrap();

    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.name, Some("Living Room TV".to_owned()));
    assert_eq!(device.device_type, DeviceType::Tv);
}

#[tokio::test]
async fn find_stale_returns_old_devices() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    // Device 1: old last_seen.
    let mut row1 = sample_device_row(DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10");
    row1.last_seen = "2026-03-06T00:00:00Z".to_owned();
    repo.insert(&row1).await.unwrap();

    // Device 2: recent last_seen.
    let mut row2 = sample_device_row(DEV2, "AA:BB:CC:DD:EE:02", "192.168.1.11");
    row2.last_seen = "2026-03-07T12:00:00Z".to_owned();
    repo.insert(&row2).await.unwrap();

    let stale = repo.find_stale("2026-03-07T00:00:00Z").await.unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].id.to_string(), DEV1);
}

#[tokio::test]
async fn find_all_returns_all_devices() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.insert(&sample_device_row(
        DEV1,
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();
    repo.insert(&sample_device_row(
        DEV2,
        "AA:BB:CC:DD:EE:02",
        "192.168.1.11",
    ))
    .await
    .unwrap();
    repo.insert(&sample_device_row(
        DEV3,
        "AA:BB:CC:DD:EE:03",
        "192.168.1.12",
    ))
    .await
    .unwrap();

    let devices = repo.find_all().await.unwrap();
    assert_eq!(devices.len(), 3);
}

#[tokio::test]
async fn find_rule_for_device_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10").await;
    sqlx::query(
        "INSERT INTO routing_rules (id, device_id, target_json, created_by) \
         VALUES ('r1', ?, '{\"type\":\"direct\"}', 'user')",
    )
    .bind(DEV1)
    .execute(&pool)
    .await
    .unwrap();

    let repo = SqliteDeviceRepository::new(pool);
    let rule = repo.find_rule_for_device(DEV1).await.unwrap().unwrap();
    assert_eq!(rule.target, RoutingTarget::Direct);
    assert_eq!(rule.created_by, RuleCreator::User);
}

#[tokio::test]
async fn find_rule_not_found() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    let result = repo.find_rule_for_device(DEV1).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn upsert_user_rule_insert_and_update() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10").await;
    let repo = SqliteDeviceRepository::new(pool);

    // Insert.
    repo.upsert_user_rule(DEV1, "{\"type\":\"direct\"}", "2026-03-07T00:00:00Z")
        .await
        .unwrap();
    let rule = repo.find_rule_for_device(DEV1).await.unwrap().unwrap();
    assert_eq!(rule.target, RoutingTarget::Direct);

    // Update (upsert).
    repo.upsert_user_rule(DEV1, "{\"type\":\"default\"}", "2026-03-07T01:00:00Z")
        .await
        .unwrap();
    let rule = repo.find_rule_for_device(DEV1).await.unwrap().unwrap();
    assert_eq!(rule.target, RoutingTarget::Default);
}

#[tokio::test]
async fn update_admin_locked_sets_flag() {
    let pool = test_pool().await;
    let repo = SqliteDeviceRepository::new(pool);

    repo.insert(&sample_device_row(
        DEV1,
        "AA:BB:CC:DD:EE:01",
        "192.168.1.10",
    ))
    .await
    .unwrap();

    // Initially unlocked.
    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert!(!device.admin_locked);

    // Lock.
    repo.update_admin_locked(DEV1, true).await.unwrap();
    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert!(device.admin_locked);

    // Unlock.
    repo.update_admin_locked(DEV1, false).await.unwrap();
    let device = repo.find_by_id(DEV1).await.unwrap().unwrap();
    assert!(!device.admin_locked);
}

#[tokio::test]
async fn count_devices() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10").await;
    insert_device(&pool, DEV2, "AA:BB:CC:DD:EE:02", "192.168.1.11").await;
    insert_device(&pool, DEV3, "AA:BB:CC:DD:EE:03", "192.168.1.12").await;
    let repo = SqliteDeviceRepository::new(pool);

    assert_eq!(repo.count().await.unwrap(), 3);
}
