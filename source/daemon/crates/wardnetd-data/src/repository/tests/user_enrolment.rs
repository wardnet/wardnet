use sqlx::SqlitePool;

use super::test_pool;
use crate::repository::sqlite::user::SqliteUserRepository;
use crate::repository::sqlite::user_enrolment::SqliteUserEnrolmentRepository;
use crate::repository::user::{UserRepository, UserRole, UserRow};
use crate::repository::user_enrolment::{EnrolmentTokenRow, UserEnrolmentRepository};

const ANN: &str = "aaaaaaaa-0000-4000-a000-000000000001";

async fn fixture() -> (SqlitePool, SqliteUserEnrolmentRepository) {
    let pool = test_pool().await;
    SqliteUserRepository::new(pool.clone())
        .create(&UserRow {
            id: ANN.to_owned(),
            display_name: "Ann".to_owned(),
            email: None,
            role: UserRole::Member,
            enabled: true,
            created_at: "2026-08-10T00:00:00Z".to_owned(),
            updated_at: "2026-08-10T00:00:00Z".to_owned(),
        })
        .await
        .unwrap();
    let repo = SqliteUserEnrolmentRepository::new(pool.clone());
    (pool, repo)
}

fn token(id: &str, token_hash: &str, expires_at: &str) -> EnrolmentTokenRow {
    EnrolmentTokenRow {
        id: id.to_owned(),
        user_id: ANN.to_owned(),
        token_hash: token_hash.to_owned(),
        created_at: "2026-08-10T00:00:00Z".to_owned(),
        expires_at: expires_at.to_owned(),
        used_at: None,
    }
}

#[tokio::test]
async fn a_fresh_token_is_redeemable() {
    let (_pool, repo) = fixture().await;
    let row = token("t1", "hash-1", "2026-08-12T00:00:00Z");
    repo.create(&row).await.unwrap();

    let found = repo
        .find_redeemable("hash-1", "2026-08-11T00:00:00Z")
        .await
        .unwrap();

    assert_eq!(found, Some(row));
}

#[tokio::test]
async fn an_expired_token_is_not_redeemable() {
    let (_pool, repo) = fixture().await;
    repo.create(&token("t1", "hash-1", "2026-08-09T00:00:00Z"))
        .await
        .unwrap();

    assert_eq!(
        repo.find_redeemable("hash-1", "2026-08-11T00:00:00Z")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn a_spent_token_is_not_redeemable() {
    let (_pool, repo) = fixture().await;
    repo.create(&token("t1", "hash-1", "2026-08-12T00:00:00Z"))
        .await
        .unwrap();
    repo.mark_used("t1", "2026-08-10T12:00:00Z").await.unwrap();

    assert_eq!(
        repo.find_redeemable("hash-1", "2026-08-11T00:00:00Z")
            .await
            .unwrap(),
        None
    );
}

/// `used_at IS NULL` lives in the `UPDATE`'s `WHERE` clause, making redemption
/// a compare-and-swap. A check-then-write in the service would let two
/// concurrent redemptions both report success.
#[tokio::test]
async fn redemption_is_single_use_even_when_repeated() {
    let (_pool, repo) = fixture().await;
    repo.create(&token("t1", "hash-1", "2026-08-12T00:00:00Z"))
        .await
        .unwrap();

    assert_eq!(
        repo.mark_used("t1", "2026-08-10T12:00:00Z").await.unwrap(),
        1
    );
    assert_eq!(
        repo.mark_used("t1", "2026-08-10T12:00:01Z").await.unwrap(),
        0,
        "the second redemption must report that it changed nothing"
    );
}

#[tokio::test]
async fn the_token_hash_is_unique() {
    let (_pool, repo) = fixture().await;
    repo.create(&token("t1", "hash-1", "2026-08-12T00:00:00Z"))
        .await
        .unwrap();

    assert!(
        repo.create(&token("t2", "hash-1", "2026-08-12T00:00:00Z"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn list_for_user_is_newest_first_and_includes_spent_tokens() {
    let (_pool, repo) = fixture().await;
    let mut older = token("t1", "hash-1", "2026-08-12T00:00:00Z");
    older.created_at = "2026-08-01T00:00:00Z".to_owned();
    let mut newer = token("t2", "hash-2", "2026-08-12T00:00:00Z");
    newer.created_at = "2026-08-09T00:00:00Z".to_owned();
    repo.create(&older).await.unwrap();
    repo.create(&newer).await.unwrap();
    repo.mark_used("t1", "2026-08-02T00:00:00Z").await.unwrap();

    let listed = repo.list_for_user(ANN).await.unwrap();

    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].id, "t2");
    assert_eq!(
        listed[1].used_at.as_deref(),
        Some("2026-08-02T00:00:00Z"),
        "a spent token stays listed so the admin sees when it was redeemed"
    );
}

#[tokio::test]
async fn delete_revokes_an_outstanding_token() {
    let (_pool, repo) = fixture().await;
    repo.create(&token("t1", "hash-1", "2026-08-12T00:00:00Z"))
        .await
        .unwrap();

    assert_eq!(repo.delete("t1").await.unwrap(), 1);
    assert_eq!(
        repo.find_redeemable("hash-1", "2026-08-11T00:00:00Z")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn delete_expired_removes_only_expired_tokens() {
    let (_pool, repo) = fixture().await;
    repo.create(&token("t1", "hash-old", "2026-08-01T00:00:00Z"))
        .await
        .unwrap();
    repo.create(&token("t2", "hash-new", "2026-09-01T00:00:00Z"))
        .await
        .unwrap();

    assert_eq!(
        repo.delete_expired("2026-08-11T00:00:00Z").await.unwrap(),
        1
    );
    assert_eq!(repo.list_for_user(ANN).await.unwrap().len(), 1);
}

#[tokio::test]
async fn tokens_cascade_when_the_user_is_deleted() {
    let (pool, repo) = fixture().await;
    repo.create(&token("t1", "hash-1", "2026-09-01T00:00:00Z"))
        .await
        .unwrap();

    SqliteUserRepository::new(pool).delete(ANN).await.unwrap();

    assert!(repo.list_for_user(ANN).await.unwrap().is_empty());
}
