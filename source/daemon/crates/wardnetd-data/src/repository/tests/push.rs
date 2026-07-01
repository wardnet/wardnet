use super::test_pool;
use crate::repository::push::{OWNER_KIND_ADMIN, OWNER_KIND_DEVICE};
use crate::repository::{NewPushSubscription, PushRepository, SqlitePushRepository};

fn sub<'a>(
    id: &'a str,
    owner_kind: &'a str,
    owner_key: &'a str,
    endpoint: &'a str,
) -> NewPushSubscription<'a> {
    NewPushSubscription {
        id,
        owner_kind,
        owner_key,
        endpoint,
        p256dh: "p256dh-key",
        auth: "auth-secret",
        created_at: "2026-07-01T00:00:00Z",
    }
}

#[tokio::test]
async fn upsert_on_duplicate_endpoint_updates_owner_and_keys() {
    let pool = test_pool().await;
    let repo = SqlitePushRepository::new(pool);

    // A device subscribes.
    repo.upsert(sub(
        "s1",
        OWNER_KIND_DEVICE,
        "aa:bb:cc:00",
        "https://push/endpoint-1",
    ))
    .await
    .unwrap();
    // The same browser (same endpoint) later subscribes as an admin, with
    // rotated keys. This must move ownership, not create a second row.
    let mut promoted = sub(
        "s2",
        OWNER_KIND_ADMIN,
        "admin-uuid",
        "https://push/endpoint-1",
    );
    promoted.p256dh = "rotated-p256dh";
    repo.upsert(promoted).await.unwrap();

    let device_subs = repo
        .list_by_owner(OWNER_KIND_DEVICE, "aa:bb:cc:00")
        .await
        .unwrap();
    assert!(device_subs.is_empty(), "old device ownership must be gone");

    let admins = repo.list_admins().await.unwrap();
    assert_eq!(admins.len(), 1);
    assert_eq!(admins[0].endpoint, "https://push/endpoint-1");
    assert_eq!(admins[0].p256dh, "rotated-p256dh");
}

#[tokio::test]
async fn list_admins_returns_only_admin_rows() {
    let pool = test_pool().await;
    let repo = SqlitePushRepository::new(pool);

    repo.upsert(sub(
        "d1",
        OWNER_KIND_DEVICE,
        "aa:bb:cc:01",
        "https://push/d1",
    ))
    .await
    .unwrap();
    repo.upsert(sub("a1", OWNER_KIND_ADMIN, "admin-1", "https://push/a1"))
        .await
        .unwrap();
    repo.upsert(sub("a2", OWNER_KIND_ADMIN, "admin-2", "https://push/a2"))
        .await
        .unwrap();

    let admins = repo.list_admins().await.unwrap();
    assert_eq!(admins.len(), 2);
    assert!(admins.iter().all(|s| s.owner_kind == OWNER_KIND_ADMIN));
}

#[tokio::test]
async fn delete_by_owner_vs_endpoint() {
    let pool = test_pool().await;
    let repo = SqlitePushRepository::new(pool);

    repo.upsert(sub("d1", OWNER_KIND_DEVICE, "mac-1", "https://push/d1"))
        .await
        .unwrap();
    repo.upsert(sub("d2", OWNER_KIND_DEVICE, "mac-1", "https://push/d2"))
        .await
        .unwrap();

    // Prune a single stale endpoint (as after a 410 Gone).
    let removed = repo.delete_by_endpoint("https://push/d1").await.unwrap();
    assert_eq!(removed, 1);
    assert_eq!(
        repo.list_by_owner(OWNER_KIND_DEVICE, "mac-1")
            .await
            .unwrap()
            .len(),
        1
    );

    // Unsubscribe removes everything the owner has left.
    let removed = repo
        .delete_by_owner(OWNER_KIND_DEVICE, "mac-1")
        .await
        .unwrap();
    assert_eq!(removed, 1);
    assert!(
        repo.list_by_owner(OWNER_KIND_DEVICE, "mac-1")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn delete_by_owner_and_endpoint_is_owner_scoped() {
    let pool = test_pool().await;
    let repo = SqlitePushRepository::new(pool);

    repo.upsert(sub(
        "a1",
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/shared",
    ))
    .await
    .unwrap();

    // A different owner supplying the same endpoint must NOT delete it.
    let removed = repo
        .delete_by_owner_and_endpoint(OWNER_KIND_DEVICE, "mac-x", "https://push/shared")
        .await
        .unwrap();
    assert_eq!(removed, 0);
    assert_eq!(repo.list_admins().await.unwrap().len(), 1);

    // The real owner can delete it.
    let removed = repo
        .delete_by_owner_and_endpoint(OWNER_KIND_ADMIN, "admin-1", "https://push/shared")
        .await
        .unwrap();
    assert_eq!(removed, 1);
    assert!(repo.list_admins().await.unwrap().is_empty());
}
