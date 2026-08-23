use super::test_pool;
use crate::repository::{
    AccessRequestRepository, DuplicateOpenAccessRequestError, SqliteAccessRequestRepository,
};
use wardnet_common::access_request::{AccessRequestKind, AccessRequestStatus};

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
    let repo = SqliteAccessRequestRepository::new(pool);

    let created = repo
        .insert(
            "r1",
            DEV1,
            AccessRequestKind::Block,
            Some("ads.example.com"),
            Some("annoying"),
            "2026-06-18T00:00:00Z",
        )
        .await
        .unwrap();
    assert_eq!(created.status, AccessRequestStatus::Pending);
    assert_eq!(created.kind, AccessRequestKind::Block);

    repo.insert(
        "r2",
        DEV2,
        AccessRequestKind::Allow,
        Some("good.com"),
        None,
        "2026-06-18T00:01:00Z",
    )
    .await
    .unwrap();

    let mine = repo.list_by_device(DEV1).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].domain.as_deref(), Some("ads.example.com"));
    assert_eq!(mine[0].reason.as_deref(), Some("annoying"));
}

#[tokio::test]
async fn list_all_filters_by_status() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteAccessRequestRepository::new(pool);

    repo.insert(
        "r1",
        DEV1,
        AccessRequestKind::Block,
        Some("a.com"),
        None,
        "2026-06-18T00:00:00Z",
    )
    .await
    .unwrap();
    repo.insert(
        "r2",
        DEV1,
        AccessRequestKind::Allow,
        Some("b.com"),
        None,
        "2026-06-18T00:01:00Z",
    )
    .await
    .unwrap();

    // Approve one; filtering by status should split them.
    repo.update_status(
        "r1",
        AccessRequestStatus::Approved,
        "admin-1",
        "2026-06-18T01:00:00Z",
    )
    .await
    .unwrap();

    let pending = repo
        .list_all(Some(AccessRequestStatus::Pending))
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, "r2");

    let approved = repo
        .list_all(Some(AccessRequestStatus::Approved))
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
    let repo = SqliteAccessRequestRepository::new(pool);

    let out = repo
        .update_status(
            "nope",
            AccessRequestStatus::Rejected,
            "admin-1",
            "2026-06-18T01:00:00Z",
        )
        .await
        .unwrap();
    assert!(out.is_none());
}

#[tokio::test]
async fn private_dns_request_carries_no_domain() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteAccessRequestRepository::new(pool);

    let created = repo
        .insert(
            "r1",
            DEV1,
            AccessRequestKind::PrivateDns,
            None,
            None,
            "2026-08-14T00:00:00Z",
        )
        .await
        .unwrap();
    assert_eq!(created.kind, AccessRequestKind::PrivateDns);
    assert!(created.domain.is_none());
}

/// The partial unique index is scoped to `private_dns` precisely so this stays
/// legal — one pending request per domain a member wants unblocked.
#[tokio::test]
async fn several_rule_requests_may_be_pending_at_once() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteAccessRequestRepository::new(pool);

    for (id, domain) in [("r1", "a.com"), ("r2", "b.com"), ("r3", "c.com")] {
        repo.insert(
            id,
            DEV1,
            AccessRequestKind::Allow,
            Some(domain),
            None,
            "2026-08-14T00:00:00Z",
        )
        .await
        .unwrap();
    }

    let pending = repo
        .list_all(Some(AccessRequestStatus::Pending))
        .await
        .unwrap();
    assert_eq!(pending.len(), 3);
}

#[tokio::test]
async fn a_second_open_private_dns_request_is_rejected() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteAccessRequestRepository::new(pool);

    repo.insert(
        "r1",
        DEV1,
        AccessRequestKind::PrivateDns,
        None,
        None,
        "2026-08-14T00:00:00Z",
    )
    .await
    .unwrap();

    let err = repo
        .insert(
            "r2",
            DEV1,
            AccessRequestKind::PrivateDns,
            None,
            None,
            "2026-08-14T00:01:00Z",
        )
        .await
        .expect_err("a second open private_dns request must be refused");
    assert!(
        err.downcast_ref::<DuplicateOpenAccessRequestError>()
            .is_some(),
        "expected the duplicate marker, got: {err}"
    );
}

/// Once decided, the index no longer applies — a declined member may ask again.
#[tokio::test]
async fn a_decided_private_dns_request_frees_the_slot() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteAccessRequestRepository::new(pool);

    repo.insert(
        "r1",
        DEV1,
        AccessRequestKind::PrivateDns,
        None,
        None,
        "2026-08-14T00:00:00Z",
    )
    .await
    .unwrap();
    repo.update_status(
        "r1",
        AccessRequestStatus::Rejected,
        "admin-1",
        "2026-08-14T00:30:00Z",
    )
    .await
    .unwrap();

    repo.insert(
        "r2",
        DEV1,
        AccessRequestKind::PrivateDns,
        None,
        None,
        "2026-08-14T01:00:00Z",
    )
    .await
    .expect("re-requesting after a decision must be allowed");
}

#[tokio::test]
async fn resolve_pending_is_idempotent_and_preserves_the_first_decider() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteAccessRequestRepository::new(pool);

    repo.insert(
        "r1",
        DEV1,
        AccessRequestKind::PrivateDns,
        None,
        None,
        "2026-08-14T00:00:00Z",
    )
    .await
    .unwrap();

    let first = repo
        .resolve_pending(
            DEV1,
            AccessRequestKind::PrivateDns,
            AccessRequestStatus::Approved,
            Some("admin-1"),
            "2026-08-14T00:05:00Z",
        )
        .await
        .unwrap()
        .expect("the pending request should resolve");
    assert_eq!(first.status, AccessRequestStatus::Approved);
    assert_eq!(first.decided_by.as_deref(), Some("admin-1"));

    // The approval path and the listener both write; the second is a no-op
    // rather than an overwrite.
    let second = repo
        .resolve_pending(
            DEV1,
            AccessRequestKind::PrivateDns,
            AccessRequestStatus::Approved,
            Some("admin-2"),
            "2026-08-14T00:06:00Z",
        )
        .await
        .unwrap();
    assert!(second.is_none(), "nothing was left pending to resolve");

    let all = repo.list_by_device(DEV1).await.unwrap();
    assert_eq!(all[0].decided_by.as_deref(), Some("admin-1"));
}

#[tokio::test]
async fn resolve_pending_ignores_other_kinds() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteAccessRequestRepository::new(pool);

    repo.insert(
        "r1",
        DEV1,
        AccessRequestKind::Allow,
        Some("a.com"),
        None,
        "2026-08-14T00:00:00Z",
    )
    .await
    .unwrap();

    let out = repo
        .resolve_pending(
            DEV1,
            AccessRequestKind::PrivateDns,
            AccessRequestStatus::Approved,
            Some("admin-1"),
            "2026-08-14T00:05:00Z",
        )
        .await
        .unwrap();
    assert!(
        out.is_none(),
        "an allow request must not be resolved by a grant"
    );
}
