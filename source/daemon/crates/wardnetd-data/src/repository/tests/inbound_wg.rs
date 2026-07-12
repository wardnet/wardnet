use super::test_pool;
use crate::repository::inbound_wg::{InboundWgPeerRepository, InboundWgPeerRow};
use crate::repository::sqlite::inbound_wg::SqliteInboundWgPeerRepository;

fn peer(id: &str, pubkey: &str, ip: &str, enabled: bool, created_at: &str) -> InboundWgPeerRow {
    InboundWgPeerRow {
        id: id.to_owned(),
        public_key: pubkey.to_owned(),
        allowed_ip: ip.to_owned(),
        name: format!("peer-{id}"),
        enabled,
        created_at: created_at.to_owned(),
        // No device linked by default (the FK requires a real device row);
        // the device-link is covered by the service-layer tests.
        device_id: None,
    }
}

/// Insert a minimal `devices` row so a peer's `device_id` FK/`UNIQUE` link
/// points at a real device — keeps the device-link tests correct even if
/// foreign-key enforcement is enabled on the pool.
async fn insert_device(pool: &sqlx::SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, ?, '10.100.64.2', 'unknown', '2026-07-05T00:00:00Z', '2026-07-05T00:00:00Z', \
         '00000000-0000-0000-0000-000000000201')",
    )
    .bind(id)
    .bind(format!("mac-{id}"))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn insert_and_find_by_id_round_trips() {
    let pool = test_pool().await;
    let repo = SqliteInboundWgPeerRepository::new(pool);

    let row = peer(
        "p1",
        "PUBKEY1",
        "10.100.64.2/32",
        true,
        "2026-07-05T00:00:00Z",
    );
    repo.insert(&row).await.unwrap();

    let found = repo.find_by_id("p1").await.unwrap().unwrap();
    assert_eq!(found, row);
    assert!(repo.find_by_id("missing").await.unwrap().is_none());
}

#[tokio::test]
async fn find_all_is_oldest_first() {
    let pool = test_pool().await;
    let repo = SqliteInboundWgPeerRepository::new(pool);

    repo.insert(&peer(
        "p2",
        "K2",
        "10.100.64.3/32",
        true,
        "2026-07-05T02:00:00Z",
    ))
    .await
    .unwrap();
    repo.insert(&peer(
        "p1",
        "K1",
        "10.100.64.2/32",
        true,
        "2026-07-05T01:00:00Z",
    ))
    .await
    .unwrap();

    let all = repo.find_all().await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, "p1");
    assert_eq!(all[1].id, "p2");
}

#[tokio::test]
async fn find_enabled_excludes_disabled() {
    let pool = test_pool().await;
    let repo = SqliteInboundWgPeerRepository::new(pool);

    repo.insert(&peer(
        "on",
        "K1",
        "10.100.64.2/32",
        true,
        "2026-07-05T01:00:00Z",
    ))
    .await
    .unwrap();
    repo.insert(&peer(
        "off",
        "K2",
        "10.100.64.3/32",
        false,
        "2026-07-05T02:00:00Z",
    ))
    .await
    .unwrap();

    let enabled = repo.find_enabled().await.unwrap();
    assert_eq!(enabled.len(), 1);
    assert_eq!(enabled[0].id, "on");
}

#[tokio::test]
async fn delete_removes_row() {
    let pool = test_pool().await;
    let repo = SqliteInboundWgPeerRepository::new(pool);

    repo.insert(&peer(
        "p1",
        "K1",
        "10.100.64.2/32",
        true,
        "2026-07-05T00:00:00Z",
    ))
    .await
    .unwrap();
    repo.delete("p1").await.unwrap();
    assert!(repo.find_by_id("p1").await.unwrap().is_none());
    // Deleting an absent id is a no-op.
    repo.delete("p1").await.unwrap();
}

#[tokio::test]
async fn public_key_uniqueness_is_enforced() {
    let pool = test_pool().await;
    let repo = SqliteInboundWgPeerRepository::new(pool);

    repo.insert(&peer(
        "p1",
        "DUP",
        "10.100.64.2/32",
        true,
        "2026-07-05T00:00:00Z",
    ))
    .await
    .unwrap();
    let err = repo
        .insert(&peer(
            "p2",
            "DUP",
            "10.100.64.3/32",
            true,
            "2026-07-05T01:00:00Z",
        ))
        .await;
    assert!(err.is_err(), "duplicate public_key must be rejected");
}

#[tokio::test]
async fn find_by_device_id_returns_linked_peer() {
    let pool = test_pool().await;
    insert_device(&pool, "device-1").await;
    let repo = SqliteInboundWgPeerRepository::new(pool);

    let mut row = peer("p1", "K1", "10.100.64.2/32", true, "2026-07-05T00:00:00Z");
    row.device_id = Some("device-1".to_owned());
    repo.insert(&row).await.unwrap();

    let found = repo.find_by_device_id("device-1").await.unwrap().unwrap();
    assert_eq!(found, row);
    assert_eq!(found.device_id.as_deref(), Some("device-1"));

    // A device with no linked peer resolves to None.
    assert!(
        repo.find_by_device_id("device-absent")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn device_id_uniqueness_is_enforced() {
    let pool = test_pool().await;
    insert_device(&pool, "device-1").await;
    let repo = SqliteInboundWgPeerRepository::new(pool);

    let mut first = peer("p1", "K1", "10.100.64.2/32", true, "2026-07-05T00:00:00Z");
    first.device_id = Some("device-1".to_owned());
    repo.insert(&first).await.unwrap();

    // Distinct id / public_key / allowed_ip, but the same device_id — the
    // one-credential-per-device UNIQUE constraint (#810) must reject it.
    let mut second = peer("p2", "K2", "10.100.64.3/32", true, "2026-07-05T01:00:00Z");
    second.device_id = Some("device-1".to_owned());
    let err = repo.insert(&second).await;
    assert!(err.is_err(), "duplicate device_id must be rejected");
}
