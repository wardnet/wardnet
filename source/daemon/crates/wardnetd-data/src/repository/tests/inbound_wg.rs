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
    }
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
