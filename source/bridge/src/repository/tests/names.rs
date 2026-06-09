//! Live-Postgres tests for the global naming authority and the two-database
//! reservation saga. These run against a real `names` table (global pool) and,
//! for the saga, a real `installs` table (regional pool) — the `UNIQUE`-as-lock
//! and the `$N` queries are runtime sqlx, invisible to compilation, so this is
//! the gate that actually proves correctness.

use chrono::{Duration, Utc};

use crate::db::DbPools;
use crate::repository::install::{Install, InstallRepository, PgInstallRepository};
use crate::repository::names::{NameRepository, PgNameRepository};
use crate::sweep::sweep_once;
use crate::test_helpers::{test_pool, test_pool_global};

/// `new()` is a trivial one-liner; call it once without `Postgres` so it shows covered.
#[tokio::test]
async fn new_from_lazy_pool() {
    let pool =
        sqlx::PgPool::connect_lazy("postgres://postgres:postgres@127.0.0.1:5432/dummy").unwrap();
    let _ = PgNameRepository::new(pool);
}

async fn names_repo() -> PgNameRepository {
    let pool = test_pool_global().await;
    PgNameRepository::new_pools(DbPools::single(pool))
}

fn future() -> chrono::DateTime<Utc> {
    Utc::now() + Duration::minutes(5)
}

#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn reserve_then_is_taken() {
    let repo = names_repo().await;
    assert!(!repo.is_taken("alice").await.unwrap());
    assert!(
        repo.reserve("alice", "id-1", "us", Utc::now(), future())
            .await
            .unwrap()
    );
    assert!(repo.is_taken("alice").await.unwrap());
}

/// Acceptance criterion 1: two concurrent registrations for the same slug —
/// exactly one wins, the other is told "taken". This is the real `UNIQUE`
/// constraint acting as the cross-region allocation lock.
#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn concurrent_reserve_one_winner() {
    let repo = names_repo().await;
    let exp = future();
    let created = Utc::now();

    let (a, b) = tokio::join!(
        repo.reserve("bob", "id-a", "us", created, exp),
        repo.reserve("bob", "id-b", "eu", created, exp),
    );

    let a = a.unwrap();
    let b = b.unwrap();
    assert!(a ^ b, "exactly one reserve must win (got a={a}, b={b})");
    assert!(repo.is_taken("bob").await.unwrap());
}

#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn confirm_marks_active_and_survives_release() {
    let repo = names_repo().await;
    repo.reserve("carol", "id-c", "us", Utc::now(), future())
        .await
        .unwrap();
    repo.confirm("carol").await.unwrap();

    // `release` only drops `reserved` rows — a confirmed name is never lost to a
    // late error on the registration path, and it reports that nothing was
    // removed (so the caller leaves the paired install row in place).
    assert!(!repo.release("carol").await.unwrap());
    assert!(repo.is_taken("carol").await.unwrap());
}

#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn confirm_errors_when_reservation_gone() {
    // If the reserved row vanished (swept/released) before confirm, confirm must
    // error rather than silently report success for an unallocated name.
    let repo = names_repo().await;
    repo.reserve("grace", "id-g", "us", Utc::now(), future())
        .await
        .unwrap();
    assert!(repo.release("grace").await.unwrap());
    assert!(repo.confirm("grace").await.is_err());
}

#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn release_frees_reserved_name() {
    let repo = names_repo().await;
    repo.reserve("dave", "id-d", "us", Utc::now(), future())
        .await
        .unwrap();
    assert!(repo.release("dave").await.unwrap(), "reserved row removed");
    assert!(!repo.is_taken("dave").await.unwrap());
    // Freed name can be re-reserved.
    assert!(
        repo.reserve("dave", "id-d2", "us", Utc::now(), future())
            .await
            .unwrap()
    );
}

/// Acceptance criterion 3: availability reflects the global registry and is
/// region-independent (a name reserved in `eu` reads as taken everywhere).
#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn availability_is_region_independent() {
    let repo = names_repo().await;
    repo.reserve("erin", "id-e", "eu", Utc::now(), future())
        .await
        .unwrap();
    assert!(repo.is_taken("erin").await.unwrap());
}

#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn sweep_is_region_scoped() {
    let repo = names_repo().await;
    let past = Utc::now() - Duration::minutes(1);
    repo.reserve("us-name", "id-us", "us", past, past)
        .await
        .unwrap();
    repo.reserve("eu-name", "id-eu", "eu", past, past)
        .await
        .unwrap();

    let reaped = repo.sweep_expired(Utc::now(), "us").await.unwrap();
    assert_eq!(reaped, vec!["id-us".to_string()]);
    assert!(!repo.is_taken("us-name").await.unwrap());
    // The other region's reservation is untouched — each bridge owns its own.
    assert!(repo.is_taken("eu-name").await.unwrap());
}

/// Acceptance criterion 2: a crashed/abandoned registration's reservation
/// expires and the name becomes registerable again — including the regional
/// install orphan, so a *same-region* retry doesn't trip `uq_installs_name`.
/// This is the two-database saga compensation proven end-to-end.
#[tokio::test]
#[ignore = "requires Postgres (docker compose up -d)"]
async fn abandoned_reservation_swept_frees_name_and_install() {
    let names = names_repo().await;
    let installs = PgInstallRepository::new_pools(DbPools::single(test_pool().await));

    // Simulate a crash between provision and confirm: a reserved (expired) name
    // plus the regional install row it created.
    let past = Utc::now() - Duration::minutes(1);
    names
        .reserve("frank", "id-f", "us", past, past)
        .await
        .unwrap();
    installs
        .insert(&sample_install("id-f", "frank"))
        .await
        .unwrap();

    // Sweep cleans BOTH databases for this region.
    let reaped = sweep_once(&names, &installs, "us").await.unwrap();
    assert_eq!(reaped, 1);
    assert!(!names.is_taken("frank").await.unwrap());
    assert!(installs.find_by_id("id-f").await.unwrap().is_none());

    // A same-region retry now succeeds — name free globally AND no install orphan
    // to collide with `uq_installs_name`.
    assert!(
        names
            .reserve("frank", "id-f2", "us", Utc::now(), future())
            .await
            .unwrap()
    );
    installs
        .insert(&sample_install("id-f2", "frank"))
        .await
        .expect("re-registration must not hit uq_installs_name");
}

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
        cf_acme_record_ids: Vec::new(),
        created_at: now,
        updated_at: now,
    }
}
