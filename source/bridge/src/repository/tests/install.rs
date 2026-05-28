use chrono::Utc;

use crate::db;
use crate::repository::install::{Install, InstallRepository, SqliteInstallRepository};

async fn repo() -> SqliteInstallRepository {
    let pools = db::init(":memory:").await.expect("in-memory db");
    SqliteInstallRepository::new_pools(pools)
}

/// A valid base64-encoded 32-byte all-zero Ed25519 public key used in tests.
const TEST_PUBLIC_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

fn sample_install(id: &str, name: &str) -> Install {
    let now = Utc::now();
    Install {
        id: id.to_string(),
        name: name.to_string(),
        public_key: TEST_PUBLIC_KEY.to_string(),
        pub_key_bytes: [0u8; 32],
        token_hash: format!("hash_{id}"),
        ip: None,
        cf_a_record_id: None,
        cf_acme_record_id: None,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test]
async fn insert_and_find_by_id() {
    let repo = repo().await;
    let install = sample_install("id-1", "happy-einstein");
    repo.insert(&install).await.unwrap();

    let found = repo
        .find_by_id("id-1")
        .await
        .unwrap()
        .expect("should exist");
    assert_eq!(found.name, "happy-einstein");
    assert_eq!(found.token_hash, "hash_id-1");
}

#[tokio::test]
async fn find_by_name() {
    let repo = repo().await;
    repo.insert(&sample_install("id-2", "brave-newton"))
        .await
        .unwrap();

    let found = repo
        .find_by_name("brave-newton")
        .await
        .unwrap()
        .expect("should exist");
    assert_eq!(found.id, "id-2");
}

#[tokio::test]
async fn find_by_token_hash() {
    let repo = repo().await;
    repo.insert(&sample_install("id-3", "calm-darwin"))
        .await
        .unwrap();

    let found = repo
        .find_by_token_hash("hash_id-3")
        .await
        .unwrap()
        .expect("should exist");
    assert_eq!(found.id, "id-3");
}

#[tokio::test]
async fn find_missing_returns_none() {
    let repo = repo().await;
    assert!(repo.find_by_id("no-such-id").await.unwrap().is_none());
}

#[tokio::test]
async fn update_ip() {
    let repo = repo().await;
    repo.insert(&sample_install("id-4", "eager-curie"))
        .await
        .unwrap();

    let now = Utc::now().to_rfc3339();
    repo.update_ip("id-4", "203.0.113.1", "cf-record-abc", &now)
        .await
        .unwrap();

    let found = repo.find_by_id("id-4").await.unwrap().unwrap();
    assert_eq!(found.ip.as_deref(), Some("203.0.113.1"));
    assert_eq!(found.cf_a_record_id.as_deref(), Some("cf-record-abc"));
}

#[tokio::test]
async fn update_acme_record_set_and_clear() {
    let repo = repo().await;
    repo.insert(&sample_install("id-5", "fair-turing"))
        .await
        .unwrap();

    let now = Utc::now().to_rfc3339();
    repo.update_acme_record("id-5", Some("cf-txt-xyz"), &now)
        .await
        .unwrap();
    let found = repo.find_by_id("id-5").await.unwrap().unwrap();
    assert_eq!(found.cf_acme_record_id.as_deref(), Some("cf-txt-xyz"));

    repo.update_acme_record("id-5", None, &now).await.unwrap();
    let found = repo.find_by_id("id-5").await.unwrap().unwrap();
    assert!(found.cf_acme_record_id.is_none());
}

#[tokio::test]
async fn delete() {
    let repo = repo().await;
    repo.insert(&sample_install("id-6", "gentle-tesla"))
        .await
        .unwrap();

    repo.delete("id-6").await.unwrap();
    assert!(repo.find_by_id("id-6").await.unwrap().is_none());
}

#[tokio::test]
async fn registration_rate_limit_log() {
    let repo = repo().await;
    let now = Utc::now().to_rfc3339();

    repo.log_registration("1.2.3.4", &now).await.unwrap();
    repo.log_registration("1.2.3.4", &now).await.unwrap();
    repo.log_registration("5.6.7.8", &now).await.unwrap();

    // Window starts one day before now — all entries should count.
    let yesterday = (Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    let count = repo
        .count_registrations_from_ip("1.2.3.4", &yesterday)
        .await
        .unwrap();
    assert_eq!(count, 2);

    // Different IP is isolated.
    let count_other = repo
        .count_registrations_from_ip("5.6.7.8", &yesterday)
        .await
        .unwrap();
    assert_eq!(count_other, 1);
}
