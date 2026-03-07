use super::test_pool;
use crate::repository::{DeviceRepository, SqliteDeviceRepository};
use wardnet_types::device::DeviceType;
use wardnet_types::routing::{RoutingTarget, RuleCreator};

const DEV1: &str = "00000000-0000-0000-0000-000000000001";
const DEV2: &str = "00000000-0000-0000-0000-000000000002";
const DEV3: &str = "00000000-0000-0000-0000-000000000003";

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
async fn count_devices() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "AA:BB:CC:DD:EE:01", "192.168.1.10").await;
    insert_device(&pool, DEV2, "AA:BB:CC:DD:EE:02", "192.168.1.11").await;
    insert_device(&pool, DEV3, "AA:BB:CC:DD:EE:03", "192.168.1.12").await;
    let repo = SqliteDeviceRepository::new(pool);

    assert_eq!(repo.count().await.unwrap(), 3);
}
