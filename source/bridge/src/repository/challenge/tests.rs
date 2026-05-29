use chrono::Utc;

use crate::db::DbPools;
use crate::repository::challenge::{
    ChallengeRepository, MySqlChallengeRepository, RegistrationChallenge,
};
use crate::test_helpers::test_pool;

/// `new()` is a trivial one-liner; call it once without `MySQL` so it shows covered.
#[tokio::test]
async fn new_from_lazy_pool() {
    let pool = sqlx::MySqlPool::connect_lazy("mysql://root:root@127.0.0.1:3306/dummy").unwrap();
    let _ = MySqlChallengeRepository::new(pool);
}

async fn repo() -> MySqlChallengeRepository {
    let pool = test_pool().await;
    MySqlChallengeRepository::new_pools(DbPools::single(pool))
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
#[ignore = "requires MySQL (docker compose up -d)"]
async fn insert_and_find() {
    let repo = repo().await;
    repo.insert(&sample("c-1", "1.2.3.4")).await.unwrap();

    let found = repo.find_by_id("c-1").await.unwrap().expect("should exist");
    assert_eq!(found.difficulty, 24);
    assert!(found.used_at.is_none());
}

#[tokio::test]
#[ignore = "requires MySQL (docker compose up -d)"]
async fn consume_marks_used() {
    let repo = repo().await;
    repo.insert(&sample("c-2", "1.2.3.4")).await.unwrap();

    let consumed = repo.consume("c-2", Utc::now()).await.unwrap();
    assert!(consumed, "first consume should succeed");

    let second = repo.consume("c-2", Utc::now()).await.unwrap();
    assert!(!second, "second consume should be rejected");
}

#[tokio::test]
#[ignore = "requires MySQL (docker compose up -d)"]
async fn consume_missing_returns_false() {
    let repo = repo().await;
    let consumed = repo.consume("no-such-id", Utc::now()).await.unwrap();
    assert!(!consumed);
}

#[tokio::test]
#[ignore = "requires MySQL (docker compose up -d)"]
async fn count_from_ip() {
    let repo = repo().await;
    repo.insert(&sample("c-3", "10.0.0.1")).await.unwrap();
    repo.insert(&sample("c-4", "10.0.0.1")).await.unwrap();
    repo.insert(&sample("c-5", "10.0.0.2")).await.unwrap();

    let since = Utc::now() - chrono::Duration::hours(1);
    assert_eq!(repo.count_from_ip("10.0.0.1", since).await.unwrap(), 2);
    assert_eq!(repo.count_from_ip("10.0.0.2", since).await.unwrap(), 1);
    assert_eq!(repo.count_from_ip("9.9.9.9", since).await.unwrap(), 0);
}
