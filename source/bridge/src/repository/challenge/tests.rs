use chrono::Utc;

use crate::db;
use crate::repository::challenge::{
    ChallengeRepository, RegistrationChallenge, SqliteChallengeRepository,
};

async fn repo() -> SqliteChallengeRepository {
    let pools = db::init(":memory:").await.expect("in-memory db");
    SqliteChallengeRepository::new_pools(pools)
}

fn sample(id: &str, ip: &str) -> RegistrationChallenge {
    let now = Utc::now();
    RegistrationChallenge {
        id: id.to_string(),
        nonce: "abcdef1234567890".repeat(4),
        difficulty: 24,
        remote_ip: ip.to_string(),
        created_at: now,
        expires_at: now + chrono::Duration::minutes(5),
        used_at: None,
    }
}

#[tokio::test]
async fn insert_and_find() {
    let repo = repo().await;
    repo.insert(&sample("c-1", "1.2.3.4")).await.unwrap();

    let found = repo.find_by_id("c-1").await.unwrap().expect("should exist");
    assert_eq!(found.difficulty, 24);
    assert!(found.used_at.is_none());
}

#[tokio::test]
async fn consume_marks_used() {
    let repo = repo().await;
    repo.insert(&sample("c-2", "1.2.3.4")).await.unwrap();

    let now = Utc::now().to_rfc3339();
    let consumed = repo.consume("c-2", &now).await.unwrap();
    assert!(consumed, "first consume should succeed");

    // Second consume must fail — already used.
    let second = repo.consume("c-2", &now).await.unwrap();
    assert!(!second, "second consume should be rejected");
}

#[tokio::test]
async fn consume_missing_returns_false() {
    let repo = repo().await;
    let consumed = repo
        .consume("no-such-id", &Utc::now().to_rfc3339())
        .await
        .unwrap();
    assert!(!consumed);
}

#[tokio::test]
async fn count_from_ip() {
    let repo = repo().await;
    let now = Utc::now().to_rfc3339();
    repo.insert(&sample("c-3", "10.0.0.1")).await.unwrap();
    repo.insert(&sample("c-4", "10.0.0.1")).await.unwrap();
    repo.insert(&sample("c-5", "10.0.0.2")).await.unwrap();

    let since = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    assert_eq!(repo.count_from_ip("10.0.0.1", &since).await.unwrap(), 2);
    assert_eq!(repo.count_from_ip("10.0.0.2", &since).await.unwrap(), 1);
    assert_eq!(repo.count_from_ip("9.9.9.9", &now).await.unwrap(), 0);
}
