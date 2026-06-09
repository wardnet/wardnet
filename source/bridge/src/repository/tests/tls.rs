use chrono::{TimeDelta, Utc};

use crate::db::DbPools;
use crate::repository::tls::{PgTlsRepository, TlsRepository};
use crate::test_helpers::test_pool;

const FQDN: &str = "bridge.svc.prod.use1.wardnet.network";

/// `new()` is a trivial one-liner; call it once without Postgres so it shows covered.
#[tokio::test]
async fn new_from_lazy_pool() {
    let pool =
        sqlx::PgPool::connect_lazy("postgres://postgres:postgres@127.0.0.1:5432/dummy").unwrap();
    let _ = PgTlsRepository::new(pool);
}

async fn repo() -> PgTlsRepository {
    let pool = test_pool().await;
    PgTlsRepository::new_pools(DbPools::single(pool))
}

#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn store_cert_inserts_then_bumps_version() {
    let repo = repo().await;
    let not_after = Utc::now() + TimeDelta::days(90);

    let v1 = repo
        .store_cert(FQDN, b"blob-1", b"nonce-1", not_after)
        .await
        .unwrap();
    assert_eq!(v1, 1, "first store starts at version 1");

    let v2 = repo
        .store_cert(FQDN, b"blob-2", b"nonce-2", not_after)
        .await
        .unwrap();
    assert_eq!(v2, 2, "re-store bumps the version");

    let row = repo.load_cert(FQDN).await.unwrap().expect("cert present");
    assert_eq!(row.version, 2);
    assert_eq!(row.sealed_blob, b"blob-2");
    assert_eq!(row.nonce, b"nonce-2");
}

#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn load_cert_absent_is_none() {
    let repo = repo().await;
    assert!(repo.load_cert("nope.example").await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn challenge_round_trip_and_expiry() {
    let repo = repo().await;
    let future = Utc::now() + TimeDelta::minutes(10);

    repo.put_challenge("tok-live", "keyauth-live", future)
        .await
        .unwrap();
    assert_eq!(
        repo.get_challenge("tok-live").await.unwrap().as_deref(),
        Some("keyauth-live")
    );

    // An already-expired token is never served.
    let past = Utc::now() - TimeDelta::minutes(1);
    repo.put_challenge("tok-expired", "keyauth-expired", past)
        .await
        .unwrap();
    assert!(repo.get_challenge("tok-expired").await.unwrap().is_none());

    // Cleanup deletes the live token; the reaper removes the expired one.
    repo.delete_challenge("tok-live").await.unwrap();
    assert!(repo.get_challenge("tok-live").await.unwrap().is_none());

    let reaped = repo.delete_expired_challenges(Utc::now()).await.unwrap();
    assert_eq!(reaped, 1, "the expired token is reaped");
}

#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn lease_is_mutually_exclusive_until_expiry() {
    let repo = repo().await;
    let until = Utc::now() + TimeDelta::minutes(5);

    // Host A wins the lease.
    assert!(repo.acquire_lease(FQDN, "host-a", until).await.unwrap());
    // Host B cannot steal a still-valid lease.
    assert!(!repo.acquire_lease(FQDN, "host-b", until).await.unwrap());

    // After A releases, B can acquire.
    repo.release_lease(FQDN, "host-a").await.unwrap();
    assert!(repo.acquire_lease(FQDN, "host-b", until).await.unwrap());
}

#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn expired_lease_can_be_stolen() {
    let repo = repo().await;

    // Host A holds a lease that already expired.
    let past = Utc::now() - TimeDelta::seconds(1);
    assert!(repo.acquire_lease(FQDN, "host-a", past).await.unwrap());

    // Host B steals it because A's lease is in the past.
    let future = Utc::now() + TimeDelta::minutes(5);
    assert!(repo.acquire_lease(FQDN, "host-b", future).await.unwrap());

    // A can no longer release what it no longer holds (no-op), and B keeps the lease.
    repo.release_lease(FQDN, "host-a").await.unwrap();
    assert!(!repo.acquire_lease(FQDN, "host-c", future).await.unwrap());
}
