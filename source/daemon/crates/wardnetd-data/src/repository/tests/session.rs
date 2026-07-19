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
        true,
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
        false,
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
        false,
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
        true,
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

#[tokio::test]
async fn delete_by_token_hash_removes_only_the_matching_session() {
    let pool = test_pool().await;
    seed_admin(&pool).await;
    let repo = SqliteSessionRepository::new(pool);

    repo.create(
        "s1",
        "admin-1",
        "hash-a",
        "2026-01-01T00:00:00Z",
        "2099-01-01T00:00:00Z",
        true,
    )
    .await
    .unwrap();
    repo.create(
        "s2",
        "admin-1",
        "hash-b",
        "2026-01-01T00:00:00Z",
        "2099-01-01T00:00:00Z",
        false,
    )
    .await
    .unwrap();

    let deleted = repo.delete_by_token_hash("hash-a").await.unwrap();
    assert_eq!(deleted, 1);

    // The deleted session no longer resolves; the other one is untouched.
    assert!(
        repo.find_admin_id_by_token_hash("hash-a", "2026-06-01T00:00:00Z")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        repo.find_admin_id_by_token_hash("hash-b", "2026-06-01T00:00:00Z")
            .await
            .unwrap(),
        Some("admin-1".to_owned())
    );

    // Deleting an unknown hash is a no-op, not an error.
    let deleted = repo.delete_by_token_hash("no-such-hash").await.unwrap();
    assert_eq!(deleted, 0);
}

#[tokio::test]
async fn find_session_for_refresh_returns_admin_id_and_flag() {
    let pool = test_pool().await;
    seed_admin(&pool).await;
    let repo = SqliteSessionRepository::new(pool);

    // remember_me=true, not expired.
    repo.create(
        "s1",
        "admin-1",
        "hash-rm",
        "2026-01-01T00:00:00Z",
        "2099-01-01T00:00:00Z",
        true,
    )
    .await
    .unwrap();
    // remember_me=false, not expired.
    repo.create(
        "s2",
        "admin-1",
        "hash-short",
        "2026-01-01T00:00:00Z",
        "2099-01-01T00:00:00Z",
        false,
    )
    .await
    .unwrap();
    // Expired.
    repo.create(
        "s3",
        "admin-1",
        "hash-expired",
        "2026-01-01T00:00:00Z",
        "2026-01-02T00:00:00Z",
        true,
    )
    .await
    .unwrap();

    let now = "2026-06-01T00:00:00Z";
    assert_eq!(
        repo.find_session_for_refresh("hash-rm", now).await.unwrap(),
        Some((
            "admin-1".to_owned(),
            true,
            "2026-01-01T00:00:00Z".to_owned()
        ))
    );
    assert_eq!(
        repo.find_session_for_refresh("hash-short", now)
            .await
            .unwrap(),
        Some((
            "admin-1".to_owned(),
            false,
            "2026-01-01T00:00:00Z".to_owned()
        ))
    );
    // Expired session returns None.
    assert_eq!(
        repo.find_session_for_refresh("hash-expired", now)
            .await
            .unwrap(),
        None
    );
    // Unknown hash returns None.
    assert_eq!(
        repo.find_session_for_refresh("no-such-hash", now)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn rotate_token_replaces_hash_and_expiry() {
    let pool = test_pool().await;
    seed_admin(&pool).await;
    let repo = SqliteSessionRepository::new(pool);

    repo.create(
        "s1",
        "admin-1",
        "old-hash",
        "2026-01-01T00:00:00Z",
        "2099-01-01T00:00:00Z",
        true,
    )
    .await
    .unwrap();

    repo.rotate_token("old-hash", "new-hash", "2099-06-01T00:00:00Z")
        .await
        .unwrap();

    // Old hash no longer resolves.
    assert!(
        repo.find_admin_id_by_token_hash("old-hash", "2026-06-01T00:00:00Z")
            .await
            .unwrap()
            .is_none()
    );

    // New hash resolves correctly.
    assert_eq!(
        repo.find_admin_id_by_token_hash("new-hash", "2026-06-01T00:00:00Z")
            .await
            .unwrap(),
        Some("admin-1".to_owned())
    );
}
