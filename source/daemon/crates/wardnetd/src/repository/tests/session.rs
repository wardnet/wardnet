use super::test_pool;
use crate::repository::{SessionRepository, SqliteSessionRepository};

async fn seed_admin(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT INTO admins (id, username, password_hash) VALUES ('admin-1', 'admin', 'hash')",
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn create_and_find_valid_session() {
    let pool = test_pool().await;
    seed_admin(&pool).await;
    let repo = SqliteSessionRepository::new(pool);

    repo.create(
        "s1",
        "admin-1",
        "tokenhash1",
        "2026-01-01T00:00:00Z",
        "2099-01-01T00:00:00Z",
    )
    .await
    .unwrap();

    let result = repo
        .find_admin_id_by_token_hash("tokenhash1", "2026-06-01T00:00:00Z")
        .await
        .unwrap();
    assert_eq!(result, Some("admin-1".to_owned()));
}

#[tokio::test]
async fn find_expired_session_returns_none() {
    let pool = test_pool().await;
    seed_admin(&pool).await;
    let repo = SqliteSessionRepository::new(pool);

    repo.create(
        "s1",
        "admin-1",
        "tokenhash1",
        "2026-01-01T00:00:00Z",
        "2026-01-02T00:00:00Z",
    )
    .await
    .unwrap();

    let result = repo
        .find_admin_id_by_token_hash("tokenhash1", "2026-02-01T00:00:00Z")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn delete_expired_removes_only_old_sessions() {
    let pool = test_pool().await;
    seed_admin(&pool).await;
    let repo = SqliteSessionRepository::new(pool);

    // Expired session.
    repo.create(
        "s1",
        "admin-1",
        "hash-old",
        "2026-01-01T00:00:00Z",
        "2026-01-02T00:00:00Z",
    )
    .await
    .unwrap();
    // Valid session.
    repo.create(
        "s2",
        "admin-1",
        "hash-new",
        "2026-01-01T00:00:00Z",
        "2099-01-01T00:00:00Z",
    )
    .await
    .unwrap();

    let deleted = repo.delete_expired("2026-06-01T00:00:00Z").await.unwrap();
    assert_eq!(deleted, 1);

    // Valid session still exists.
    let result = repo
        .find_admin_id_by_token_hash("hash-new", "2026-06-01T00:00:00Z")
        .await
        .unwrap();
    assert!(result.is_some());
}
