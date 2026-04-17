use super::test_pool;
use crate::repository::tunnel::TunnelRow;
use crate::repository::{SqliteTunnelRepository, TunnelRepository};
use wardnet_common::tunnel::TunnelStatus;

fn sample_row(id: &str, interface_name: &str) -> TunnelRow {
    TunnelRow {
        id: id.to_owned(),
        label: "Sweden VPN".to_owned(),
        country_code: "SE".to_owned(),
        provider: Some("Mullvad".to_owned()),
        interface_name: interface_name.to_owned(),
        endpoint: "198.51.100.1:51820".to_owned(),
        status: "down".to_owned(),
        address: "[\"10.66.0.2/32\"]".to_owned(),
        dns: "[\"1.1.1.1\"]".to_owned(),
        peer_config: "{\"public_key\":\"abc123\",\"endpoint\":\"198.51.100.1:51820\",\"allowed_ips\":[\"0.0.0.0/0\"],\"preshared_key\":null,\"persistent_keepalive\":25}".to_owned(),
        listen_port: None,
    }
}

#[tokio::test]
async fn insert_and_find_by_id() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000001";

    repo.insert(&sample_row(id, "wg_ward0")).await.unwrap();

    let tunnel = repo.find_by_id(id).await.unwrap().unwrap();
    assert_eq!(tunnel.id.to_string(), id);
    assert_eq!(tunnel.label, "Sweden VPN");
    assert_eq!(tunnel.country_code, "SE");
    assert_eq!(tunnel.provider, Some("Mullvad".to_owned()));
    assert_eq!(tunnel.interface_name, "wg_ward0");
    assert_eq!(tunnel.endpoint, "198.51.100.1:51820");
    assert_eq!(tunnel.status, TunnelStatus::Down);
    assert!(tunnel.last_handshake.is_none());
    assert_eq!(tunnel.bytes_tx, 0);
    assert_eq!(tunnel.bytes_rx, 0);
}

#[tokio::test]
async fn find_all_returns_all() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);

    repo.insert(&sample_row(
        "00000000-0000-0000-0000-000000000001",
        "wg_ward0",
    ))
    .await
    .unwrap();
    repo.insert(&sample_row(
        "00000000-0000-0000-0000-000000000002",
        "wg_ward1",
    ))
    .await
    .unwrap();

    let tunnels = repo.find_all().await.unwrap();
    assert_eq!(tunnels.len(), 2);
}

#[tokio::test]
async fn find_by_id_returns_none_for_missing() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);

    let result = repo
        .find_by_id("00000000-0000-0000-0000-000000000099")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn update_status() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000001";

    repo.insert(&sample_row(id, "wg_ward0")).await.unwrap();
    repo.update_status(id, "up").await.unwrap();

    let tunnel = repo.find_by_id(id).await.unwrap().unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Up);
}

#[tokio::test]
async fn update_stats() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000001";

    repo.insert(&sample_row(id, "wg_ward0")).await.unwrap();
    repo.update_stats(id, 1024, 2048, Some("2026-03-07T12:00:00Z"))
        .await
        .unwrap();

    let tunnel = repo.find_by_id(id).await.unwrap().unwrap();
    assert_eq!(tunnel.bytes_tx, 1024);
    assert_eq!(tunnel.bytes_rx, 2048);
    assert!(tunnel.last_handshake.is_some());
}

#[tokio::test]
async fn delete() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000001";

    repo.insert(&sample_row(id, "wg_ward0")).await.unwrap();
    repo.delete(id).await.unwrap();

    let result = repo.find_by_id(id).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn next_interface_index_empty_table() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);

    let idx = repo.next_interface_index().await.unwrap();
    assert_eq!(idx, 0);
}

#[tokio::test]
async fn next_interface_index_increments() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);

    repo.insert(&sample_row(
        "00000000-0000-0000-0000-000000000001",
        "wg_ward0",
    ))
    .await
    .unwrap();
    assert_eq!(repo.next_interface_index().await.unwrap(), 1);

    repo.insert(&sample_row(
        "00000000-0000-0000-0000-000000000002",
        "wg_ward1",
    ))
    .await
    .unwrap();
    assert_eq!(repo.next_interface_index().await.unwrap(), 2);
}

#[tokio::test]
async fn count() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);

    repo.insert(&sample_row(
        "00000000-0000-0000-0000-000000000001",
        "wg_ward0",
    ))
    .await
    .unwrap();
    repo.insert(&sample_row(
        "00000000-0000-0000-0000-000000000002",
        "wg_ward1",
    ))
    .await
    .unwrap();
    repo.insert(&sample_row(
        "00000000-0000-0000-0000-000000000003",
        "wg_ward2",
    ))
    .await
    .unwrap();

    assert_eq!(repo.count().await.unwrap(), 3);
}

#[tokio::test]
async fn find_config_by_id_returns_config() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000001";

    repo.insert(&sample_row(id, "wg_ward0")).await.unwrap();

    let config = repo.find_config_by_id(id).await.unwrap().unwrap();
    assert_eq!(config.address, vec!["10.66.0.2/32"]);
    assert_eq!(config.dns, vec!["1.1.1.1"]);
    assert!(config.listen_port.is_none());
    assert_eq!(config.peer.public_key, "abc123");
    assert_eq!(config.peer.allowed_ips, vec!["0.0.0.0/0"]);
    assert_eq!(config.peer.persistent_keepalive, Some(25));
}

#[tokio::test]
async fn find_config_by_id_returns_none_for_missing() {
    let pool = test_pool().await;
    let repo = SqliteTunnelRepository::new(pool);

    let result = repo
        .find_config_by_id("00000000-0000-0000-0000-000000000099")
        .await
        .unwrap();
    assert!(result.is_none());
}
