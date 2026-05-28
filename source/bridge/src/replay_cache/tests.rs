use super::ReplayCache;

#[test]
fn first_insert_returns_false() {
    let cache = ReplayCache::new();
    assert!(!cache.contains_or_insert("key-1", 1_000_000));
}

#[test]
fn second_insert_returns_true() {
    let cache = ReplayCache::new();
    cache.contains_or_insert("key-2", 1_000_000);
    assert!(cache.contains_or_insert("key-2", 1_000_001));
}

#[test]
fn different_keys_are_independent() {
    let cache = ReplayCache::new();
    cache.contains_or_insert("key-a", 1_000_000);
    assert!(!cache.contains_or_insert("key-b", 1_000_000));
}

#[test]
fn expired_entries_are_pruned() {
    let cache = ReplayCache::new();
    // Insert at t=0
    cache.contains_or_insert("key-3", 0);
    // Check at t=121 — entry should have expired (window is 120 s)
    let is_replay = cache.contains_or_insert("key-3", 121);
    assert!(!is_replay, "entry should have expired and been pruned");
}

#[test]
fn unexpired_entries_are_retained() {
    let cache = ReplayCache::new();
    cache.contains_or_insert("key-4", 1_000_000);
    // 60 s later — still within the 120 s window
    assert!(cache.contains_or_insert("key-4", 1_000_060));
}
