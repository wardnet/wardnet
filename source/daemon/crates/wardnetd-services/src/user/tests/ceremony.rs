//! Tests for [`CeremonyStore`] — the single-use, expiring nonce store both
//! OAuth and WebAuthn hang their round-trips on.

use std::time::Duration;

use crate::user::ceremony::CeremonyStore;

#[tokio::test]
async fn a_stored_value_can_be_taken_once() {
    let store: CeremonyStore<&str> = CeremonyStore::new();
    store.insert("state-1".to_owned(), "verifier");

    assert_eq!(store.take("state-1"), Some("verifier"));
    assert_eq!(
        store.take("state-1"),
        None,
        "take removes the entry; a second attempt is a replay and must fail"
    );
}

#[tokio::test]
async fn an_unknown_key_yields_nothing() {
    let store: CeremonyStore<&str> = CeremonyStore::new();
    assert_eq!(store.take("never-issued"), None);
}

#[tokio::test]
async fn an_expired_entry_is_refused() {
    // The TTL is what stops an abandoned ceremony staying open indefinitely.
    let store: CeremonyStore<&str> = CeremonyStore::with_ttl(Duration::from_millis(50));
    store.insert("state-1".to_owned(), "verifier");

    tokio::time::sleep(Duration::from_millis(80)).await;

    assert_eq!(
        store.take("state-1"),
        None,
        "an entry past its TTL must not be redeemable"
    );
}

#[tokio::test]
async fn taking_is_what_makes_it_single_use_even_on_a_later_failure() {
    // The entry is removed on `take`, before the caller decides whether the rest
    // of the ceremony succeeded. That ordering is deliberate: it means a failed
    // second step cannot be retried with the same nonce.
    let store: CeremonyStore<u32> = CeremonyStore::new();
    store.insert("state-1".to_owned(), 42);

    let taken = store.take("state-1");
    assert_eq!(taken, Some(42));
    assert!(store.is_empty());
}

#[tokio::test]
async fn inserting_prunes_expired_entries() {
    // Otherwise a stream of abandoned ceremonies grows the map without bound,
    // which on a Raspberry Pi is a reachable memory-exhaustion vector.
    let store: CeremonyStore<&str> = CeremonyStore::with_ttl(Duration::from_millis(30));
    store.insert("old-1".to_owned(), "a");
    store.insert("old-2".to_owned(), "b");
    assert_eq!(store.len(), 2);

    tokio::time::sleep(Duration::from_millis(60)).await;
    store.insert("fresh".to_owned(), "c");

    assert_eq!(
        store.len(),
        1,
        "the two expired entries should have been pruned by the insert"
    );
    assert_eq!(store.take("fresh"), Some("c"));
}

#[tokio::test]
async fn concurrent_ceremonies_are_independent() {
    let store: CeremonyStore<&str> = CeremonyStore::new();
    store.insert("state-a".to_owned(), "verifier-a");
    store.insert("state-b".to_owned(), "verifier-b");

    assert_eq!(store.take("state-a"), Some("verifier-a"));
    assert_eq!(
        store.take("state-b"),
        Some("verifier-b"),
        "taking one ceremony must not disturb another"
    );
}

#[tokio::test]
async fn the_store_is_bounded() {
    // A flood of unauthenticated ceremony starts must not grow memory without
    // limit. The cap is enforced on insert; the newest entry always survives.
    let store: CeremonyStore<u32> = CeremonyStore::new();
    for i in 0..400 {
        store.insert(format!("state-{i}"), i);
    }

    assert!(
        store.len() <= 256,
        "the store must stay bounded, got {}",
        store.len()
    );
    assert_eq!(
        store.take("state-399"),
        Some(399),
        "the most recent ceremony must not be the one evicted"
    );
}
