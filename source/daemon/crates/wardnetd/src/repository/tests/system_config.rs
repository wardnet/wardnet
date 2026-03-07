use super::test_pool;
use crate::repository::{SqliteSystemConfigRepository, SystemConfigRepository};

#[tokio::test]
async fn get_seeded_value() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    let version = repo.get("daemon_version").await.unwrap();
    assert_eq!(version, Some("0.1.0".to_owned()));
}

#[tokio::test]
async fn get_not_found() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    let result = repo.get("nonexistent_key").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn set_and_get() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    repo.set("my_key", "my_value").await.unwrap();
    let result = repo.get("my_key").await.unwrap();
    assert_eq!(result, Some("my_value".to_owned()));
}

#[tokio::test]
async fn set_upsert_overwrites() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    repo.set("key", "first").await.unwrap();
    repo.set("key", "second").await.unwrap();

    let result = repo.get("key").await.unwrap();
    assert_eq!(result, Some("second".to_owned()));
}

#[tokio::test]
async fn device_count_empty() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    assert_eq!(repo.device_count().await.unwrap(), 0);
}

#[tokio::test]
async fn tunnel_count_empty() {
    let pool = test_pool().await;
    let repo = SqliteSystemConfigRepository::new(pool);

    assert_eq!(repo.tunnel_count().await.unwrap(), 0);
}

#[tokio::test]
async fn device_count_with_data() {
    let pool = test_pool().await;
    let now = "2026-03-07T00:00:00Z";
    for i in 0..3 {
        sqlx::query(
            "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen) \
             VALUES (?, ?, ?, 'unknown', ?, ?)",
        )
        .bind(format!("00000000-0000-0000-0000-00000000000{i}"))
        .bind(format!("AA:BB:CC:DD:EE:{i:02X}"))
        .bind(format!("192.168.1.{}", 10 + i))
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    }

    let repo = SqliteSystemConfigRepository::new(pool);
    assert_eq!(repo.device_count().await.unwrap(), 3);
}
