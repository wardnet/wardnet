use super::test_pool;
use crate::repository::{ApiKeyRepository, SqliteApiKeyRepository};

#[tokio::test]
async fn create_and_find_all() {
    let pool = test_pool().await;
    let repo = SqliteApiKeyRepository::new(pool);

    repo.create("k1", "key-one", "hash1", "2026-01-01T00:00:00Z")
        .await
        .unwrap();
    repo.create("k2", "key-two", "hash2", "2026-01-01T00:00:00Z")
        .await
        .unwrap();

    let keys = repo.find_all_hashes().await.unwrap();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&("k1".to_owned(), "hash1".to_owned())));
    assert!(keys.contains(&("k2".to_owned(), "hash2".to_owned())));
}

#[tokio::test]
async fn find_all_empty() {
    let pool = test_pool().await;
    let repo = SqliteApiKeyRepository::new(pool);

    let keys = repo.find_all_hashes().await.unwrap();
    assert!(keys.is_empty());
}

#[tokio::test]
async fn update_last_used_succeeds() {
    let pool = test_pool().await;
    let repo = SqliteApiKeyRepository::new(pool.clone());

    repo.create("k1", "key-one", "hash1", "2026-01-01T00:00:00Z")
        .await
        .unwrap();
    repo.update_last_used("k1", "2026-06-15T12:00:00Z")
        .await
        .unwrap();

    let last_used =
        sqlx::query_scalar::<_, Option<String>>("SELECT last_used_at FROM api_keys WHERE id = ?")
            .bind("k1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(last_used, Some("2026-06-15T12:00:00Z".to_owned()));
}
