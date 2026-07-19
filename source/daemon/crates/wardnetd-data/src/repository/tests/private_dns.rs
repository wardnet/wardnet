use super::test_pool;
use crate::repository::private_dns::{
    DeviceAlreadyGrantedPrivateDnsError, PrivateDnsGrantRepository, PrivateDnsGrantRow,
};
use crate::repository::sqlite::private_dns::SqlitePrivateDnsGrantRepository;

fn grant(id: &str, device_id: &str, token: &str, created_at: &str) -> PrivateDnsGrantRow {
    PrivateDnsGrantRow {
        id: id.to_owned(),
        device_id: device_id.to_owned(),
        token: token.to_owned(),
        created_at: created_at.to_owned(),
    }
}

/// Insert a minimal `devices` row so a grant's `device_id` FK points at a
/// real device — foreign-key enforcement is on for the pool.
async fn insert_device(pool: &sqlx::SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, ?, '192.168.1.20', 'unknown', '2026-07-19T00:00:00Z', '2026-07-19T00:00:00Z', \
         '00000000-0000-0000-0000-000000000201')",
    )
    .bind(id)
    .bind(format!("mac-{id}"))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn insert_and_find_round_trips() {
    let pool = test_pool().await;
    insert_device(&pool, "device-1").await;
    let repo = SqlitePrivateDnsGrantRepository::new(pool);

    let row = grant("g1", "device-1", "mgrmzsc2ytemzsg4", "2026-07-19T00:00:00Z");
    repo.insert(&row).await.unwrap();

    let by_id = repo.find_by_id("g1").await.unwrap().unwrap();
    assert_eq!(by_id, row);
    let by_device = repo.find_by_device_id("device-1").await.unwrap().unwrap();
    assert_eq!(by_device, row);
    let by_token = repo
        .find_by_token("mgrmzsc2ytemzsg4")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(by_token, row);

    assert!(repo.find_by_id("missing").await.unwrap().is_none());
    assert!(repo.find_by_device_id("absent").await.unwrap().is_none());
    assert!(repo.find_by_token("unknown-token").await.unwrap().is_none());
}

#[tokio::test]
async fn find_all_is_oldest_first() {
    let pool = test_pool().await;
    insert_device(&pool, "device-1").await;
    insert_device(&pool, "device-2").await;
    let repo = SqlitePrivateDnsGrantRepository::new(pool);

    repo.insert(&grant("g2", "device-2", "token2", "2026-07-19T02:00:00Z"))
        .await
        .unwrap();
    repo.insert(&grant("g1", "device-1", "token1", "2026-07-19T01:00:00Z"))
        .await
        .unwrap();

    let all = repo.find_all().await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].id, "g1");
    assert_eq!(all[1].id, "g2");
}

#[tokio::test]
async fn delete_removes_row() {
    let pool = test_pool().await;
    insert_device(&pool, "device-1").await;
    let repo = SqlitePrivateDnsGrantRepository::new(pool);

    repo.insert(&grant("g1", "device-1", "token1", "2026-07-19T00:00:00Z"))
        .await
        .unwrap();
    repo.delete("g1").await.unwrap();
    assert!(repo.find_by_id("g1").await.unwrap().is_none());
    // Deleting an absent id is a no-op.
    repo.delete("g1").await.unwrap();
}

#[tokio::test]
async fn device_id_uniqueness_maps_to_marker_error() {
    let pool = test_pool().await;
    insert_device(&pool, "device-1").await;
    let repo = SqlitePrivateDnsGrantRepository::new(pool);

    repo.insert(&grant("g1", "device-1", "token1", "2026-07-19T00:00:00Z"))
        .await
        .unwrap();

    // Distinct id / token, but the same device_id — the one-grant-per-device
    // UNIQUE constraint must reject it with the downcastable marker.
    let err = repo
        .insert(&grant("g2", "device-1", "token2", "2026-07-19T01:00:00Z"))
        .await
        .unwrap_err();
    assert!(
        err.downcast_ref::<DeviceAlreadyGrantedPrivateDnsError>()
            .is_some(),
        "duplicate device_id must surface the marker error, got: {err}"
    );
}

#[tokio::test]
async fn token_uniqueness_is_an_ordinary_error() {
    let pool = test_pool().await;
    insert_device(&pool, "device-1").await;
    insert_device(&pool, "device-2").await;
    let repo = SqlitePrivateDnsGrantRepository::new(pool);

    repo.insert(&grant("g1", "device-1", "DUP", "2026-07-19T00:00:00Z"))
        .await
        .unwrap();
    let err = repo
        .insert(&grant("g2", "device-2", "DUP", "2026-07-19T01:00:00Z"))
        .await
        .unwrap_err();
    assert!(
        err.downcast_ref::<DeviceAlreadyGrantedPrivateDnsError>()
            .is_none(),
        "a token collision is an internal fault, not the device-conflict marker"
    );
}

#[tokio::test]
async fn deleting_the_device_cascades_the_grant() {
    let pool = test_pool().await;
    insert_device(&pool, "device-1").await;
    let repo = SqlitePrivateDnsGrantRepository::new(pool.clone());

    repo.insert(&grant("g1", "device-1", "token1", "2026-07-19T00:00:00Z"))
        .await
        .unwrap();

    sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind("device-1")
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        repo.find_by_id("g1").await.unwrap().is_none(),
        "grant must not outlive its device"
    );
}

#[test]
fn marker_error_displays_a_message() {
    // The service layer downcasts on this marker; its Display feeds the
    // 409 path's log line.
    let msg = DeviceAlreadyGrantedPrivateDnsError.to_string();
    assert!(msg.contains("Private DNS grant"), "got: {msg}");
}

#[tokio::test]
async fn missing_device_is_rejected_by_the_fk() {
    let pool = test_pool().await;
    let repo = SqlitePrivateDnsGrantRepository::new(pool);

    let err = repo
        .insert(&grant(
            "g1",
            "device-ghost",
            "token1",
            "2026-07-19T00:00:00Z",
        ))
        .await;
    assert!(
        err.is_err(),
        "grant for an unmanaged device must be rejected"
    );
}
