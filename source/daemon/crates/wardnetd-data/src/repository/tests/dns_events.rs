use super::test_pool;
use crate::repository::{DnsEventRow, DnsEventsRepository, SqliteDnsEventsRepository};

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
async fn insert_and_stats_for_device() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    repo.insert(DEV1, "example.com", "allowed", "2026-06-01T00:00:00Z")
        .await
        .unwrap();
    repo.insert(DEV1, "ads.tracker.io", "blocked", "2026-06-01T00:01:00Z")
        .await
        .unwrap();
    repo.insert(DEV1, "api.example.com", "allowed", "2026-06-01T00:02:00Z")
        .await
        .unwrap();

    let stats = repo.stats_for_device(DEV1).await.unwrap();
    assert_eq!(stats.row_count, 3);
    assert!(stats.size_bytes > 0);
}

#[tokio::test]
async fn stats_for_device_empty_returns_zero() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    let stats = repo.stats_for_device(DEV1).await.unwrap();
    assert_eq!(stats.row_count, 0);
    assert_eq!(stats.size_bytes, 0);
}

#[tokio::test]
async fn prune_for_device_by_age_deletes_old_rows() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    // Old row far in the past — should be pruned.
    repo.insert(DEV1, "old.example.com", "allowed", "2020-01-01T00:00:00Z")
        .await
        .unwrap();
    // Recent row — should survive.
    repo.insert(DEV1, "new.example.com", "allowed", "2099-01-01T00:00:00Z")
        .await
        .unwrap();

    // Prune with cap_days=1 (anything older than 1 day is stale), cap_count=1000.
    let deleted = repo.prune_for_device(DEV1, 1000, 1).await.unwrap();
    assert!(deleted >= 1, "expected at least the old row to be deleted");

    let stats = repo.stats_for_device(DEV1).await.unwrap();
    assert_eq!(
        stats.row_count, 1,
        "only the future-dated row should remain"
    );
}

#[tokio::test]
async fn prune_for_device_by_count_keeps_newest_rows() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    // Insert 5 rows with distinct timestamps so ordering is deterministic.
    for i in 1u8..=5 {
        let ts = format!("2099-01-0{i}T00:00:00Z");
        let domain = format!("host{i}.example.com");
        repo.insert(DEV1, &domain, "allowed", &ts).await.unwrap();
    }

    // Prune keeping only the 2 newest; age cap is irrelevant (far future dates).
    let deleted = repo.prune_for_device(DEV1, 2, 36500).await.unwrap();
    assert_eq!(deleted, 3, "3 of 5 rows should be pruned");

    let stats = repo.stats_for_device(DEV1).await.unwrap();
    assert_eq!(stats.row_count, 2, "exactly 2 rows should remain");
}

#[tokio::test]
async fn delete_all_for_device_leaves_other_devices_intact() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    insert_device(&pool, DEV2, "aa:bb:cc:dd:ee:02", "192.168.1.11").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    repo.insert(DEV1, "dev1.example.com", "allowed", "2026-06-01T00:00:00Z")
        .await
        .unwrap();
    repo.insert(DEV1, "dev1b.example.com", "blocked", "2026-06-01T00:01:00Z")
        .await
        .unwrap();
    repo.insert(DEV2, "dev2.example.com", "allowed", "2026-06-01T00:02:00Z")
        .await
        .unwrap();

    let deleted = repo.delete_all_for_device(DEV1).await.unwrap();
    assert_eq!(deleted, 2);

    let stats_dev1 = repo.stats_for_device(DEV1).await.unwrap();
    assert_eq!(stats_dev1.row_count, 0, "DEV1 rows should all be gone");

    let stats_dev2 = repo.stats_for_device(DEV2).await.unwrap();
    assert_eq!(stats_dev2.row_count, 1, "DEV2 row should be unaffected");
}

#[tokio::test]
async fn find_device_ids_with_data_returns_devices_with_events() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    insert_device(&pool, DEV2, "aa:bb:cc:dd:ee:02", "192.168.1.11").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    repo.insert(DEV1, "dev1.example.com", "allowed", "2026-06-01T00:00:00Z")
        .await
        .unwrap();
    repo.insert(DEV2, "dev2.example.com", "blocked", "2026-06-01T00:01:00Z")
        .await
        .unwrap();

    let mut ids = repo.find_device_ids_with_data().await.unwrap();
    ids.sort();
    assert_eq!(ids, vec![DEV1, DEV2]);

    // After deleting all DEV1 events, only DEV2 should appear.
    repo.delete_all_for_device(DEV1).await.unwrap();

    let ids_after = repo.find_device_ids_with_data().await.unwrap();
    assert_eq!(ids_after, vec![DEV2]);
}

#[tokio::test]
async fn insert_returns_autoincrement_id() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    let id1 = repo
        .insert(DEV1, "first.example.com", "allowed", "2026-06-12T00:00:00Z")
        .await
        .unwrap();
    let id2 = repo
        .insert(
            DEV1,
            "second.example.com",
            "blocked",
            "2026-06-12T00:01:00Z",
        )
        .await
        .unwrap();

    assert!(id1 > 0, "first inserted ID should be positive");
    assert!(id2 > id1, "IDs must be strictly increasing");
}

#[tokio::test]
async fn fetch_pending_returns_rows_oldest_first() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    repo.insert(DEV1, "a.example.com", "allowed", "2026-06-12T00:00:00Z")
        .await
        .unwrap();
    repo.insert(DEV1, "b.example.com", "blocked", "2026-06-12T00:01:00Z")
        .await
        .unwrap();
    repo.insert(DEV1, "c.example.com", "allowed", "2026-06-12T00:02:00Z")
        .await
        .unwrap();

    let rows: Vec<DnsEventRow> = repo.fetch_pending(DEV1, 0, 100).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert!(
        rows[0].id < rows[1].id && rows[1].id < rows[2].id,
        "rows must be ordered oldest-first"
    );
    assert_eq!(rows[0].domain, "a.example.com");
    assert_eq!(rows[2].domain, "c.example.com");
}

#[tokio::test]
async fn fetch_pending_respects_after_id_cursor() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    let id1 = repo
        .insert(DEV1, "a.example.com", "allowed", "2026-06-12T00:00:00Z")
        .await
        .unwrap();
    repo.insert(DEV1, "b.example.com", "allowed", "2026-06-12T00:01:00Z")
        .await
        .unwrap();

    // Only rows AFTER id1 should be returned.
    let rows = repo.fetch_pending(DEV1, id1, 100).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].domain, "b.example.com");
}

#[tokio::test]
async fn fetch_pending_empty_after_delete_up_to() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    repo.insert(DEV1, "a.example.com", "allowed", "2026-06-12T00:00:00Z")
        .await
        .unwrap();
    let id = repo
        .insert(DEV1, "b.example.com", "blocked", "2026-06-12T00:01:00Z")
        .await
        .unwrap();

    let deleted = repo.delete_up_to(DEV1, id).await.unwrap();
    assert_eq!(deleted, 2, "both rows should be deleted on ack");

    let rows = repo.fetch_pending(DEV1, 0, 100).await.unwrap();
    assert!(rows.is_empty(), "no pending rows after delete_up_to");
}

#[tokio::test]
async fn delete_up_to_scoped_to_device() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    insert_device(&pool, DEV2, "aa:bb:cc:dd:ee:02", "192.168.1.11").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    let id1 = repo
        .insert(DEV1, "dev1.example.com", "allowed", "2026-06-12T00:00:00Z")
        .await
        .unwrap();
    repo.insert(DEV2, "dev2.example.com", "allowed", "2026-06-12T00:00:00Z")
        .await
        .unwrap();

    let deleted = repo.delete_up_to(DEV1, id1).await.unwrap();
    assert_eq!(deleted, 1, "only DEV1's row should be deleted");

    let stats_dev2 = repo.stats_for_device(DEV2).await.unwrap();
    assert_eq!(stats_dev2.row_count, 1, "DEV2 row must be unaffected");
}

#[tokio::test]
async fn delete_up_to_returns_affected_count() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01", "192.168.1.10").await;
    let repo = SqliteDnsEventsRepository::new(pool);

    repo.insert(DEV1, "a.example.com", "allowed", "2026-06-12T00:00:00Z")
        .await
        .unwrap();
    repo.insert(DEV1, "b.example.com", "allowed", "2026-06-12T00:01:00Z")
        .await
        .unwrap();
    let id3 = repo
        .insert(DEV1, "c.example.com", "allowed", "2026-06-12T00:02:00Z")
        .await
        .unwrap();

    let deleted = repo.delete_up_to(DEV1, id3).await.unwrap();
    assert_eq!(deleted, 3, "all 3 rows should be deleted");

    // Calling again is idempotent — the rows are already gone.
    let deleted2 = repo.delete_up_to(DEV1, id3).await.unwrap();
    assert_eq!(deleted2, 0, "second call should delete 0 rows");
}
