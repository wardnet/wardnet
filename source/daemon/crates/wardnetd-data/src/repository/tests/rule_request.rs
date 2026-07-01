use super::test_pool;
use crate::repository::{RuleRequestRepository, SqliteRuleRequestRepository};
use wardnet_common::rule_request::{RuleRequestKind, RuleRequestStatus};

const DEV1: &str = "00000000-0000-0000-0000-000000000001";
const DEV2: &str = "00000000-0000-0000-0000-000000000002";

async fn insert_device(pool: &sqlx::SqlitePool, id: &str, mac: &str, ip: &str) {
    let now = "2026-03-07T00:00:00Z";
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, ?, ?, 'unknown', ?, ?, '00000000-0000-0000-0000-000000000201')",
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
async fn insert_then_list_by_device() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    insert_device(&pool, DEV2, "aa:bb:cc:dd:ee:02", "192.168.1.11").await;
    let repo = SqliteRuleRequestRepository::new(pool);

    let created = repo
        .insert(
            "r1",
            DEV1,
            RuleRequestKind::Block,
            "ads.example.com",
            Some("annoying"),
            "2026-06-18T00:00:00Z",
        )
        .await
        .unwrap();
    assert_eq!(created.status, RuleRequestStatus::Pending);
    assert_eq!(created.kind, RuleRequestKind::Block);

    repo.insert(
        "r2",
        DEV2,
        RuleRequestKind::Allow,
        "good.com",
        None,
        "2026-06-18T00:01:00Z",
    )
    .await
    .unwrap();

    let mine = repo.list_by_device(DEV1).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].domain, "ads.example.com");
    assert_eq!(mine[0].reason.as_deref(), Some("annoying"));
}

#[tokio::test]
async fn list_all_filters_by_status() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteRuleRequestRepository::new(pool);

    repo.insert(
        "r1",
        DEV1,
        RuleRequestKind::Block,
        "a.com",
        None,
        "2026-06-18T00:00:00Z",
    )
    .await
    .unwrap();
    repo.insert(
        "r2",
        DEV1,
        RuleRequestKind::Allow,
        "b.com",
        None,
        "2026-06-18T00:01:00Z",
    )
    .await
    .unwrap();

    // Approve one; filtering by status should split them.
    repo.update_status(
        "r1",
        RuleRequestStatus::Approved,
        "admin-1",
        "2026-06-18T01:00:00Z",
    )
    .await
    .unwrap();

    let pending = repo
        .list_all(Some(RuleRequestStatus::Pending))
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "r2");

    let approved = repo
        .list_all(Some(RuleRequestStatus::Approved))
        .await
        .unwrap();
    assert_eq!(approved.len(), 1);
    assert_eq!(approved[0].id, "r1");
    assert_eq!(approved[0].decided_by.as_deref(), Some("admin-1"));

    let all = repo.list_all(None).await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn update_status_unknown_id_returns_none() {
    let pool = test_pool().await;
    let repo = SqliteRuleRequestRepository::new(pool);

    let out = repo
        .update_status(
            "nope",
            RuleRequestStatus::Rejected,
            "admin-1",
            "2026-06-18T01:00:00Z",
        )
        .await
        .unwrap();
    assert!(out.is_none());
}
