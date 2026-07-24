//! Tests for the per-client DNS token-bucket rate limiter.

use crate::dns::rate_limit::RateLimiter;

use std::time::{Duration, Instant};

const IP: std::net::IpAddr = std::net::IpAddr::V4(std::net::Ipv4Addr::new(10, 0, 0, 5));

#[test]
fn rate_zero_always_allows() {
    let rl = RateLimiter::new(0);
    let t = Instant::now();
    for _ in 0..1000 {
        assert!(rl.check_at(IP, 0, t));
    }
}

#[test]
fn burst_then_denied_within_same_instant() {
    let rl = RateLimiter::new(0);
    let t = Instant::now();
    // Capacity = 3: first three allowed, fourth denied (no refill).
    assert!(rl.check_at(IP, 3, t));
    assert!(rl.check_at(IP, 3, t));
    assert!(rl.check_at(IP, 3, t));
    assert!(!rl.check_at(IP, 3, t));
}

#[test]
fn check_uses_atomic_rate_and_set_rate_updates_it() {
    let rl = RateLimiter::new(0);
    // rate 0 → unlimited.
    for _ in 0..100 {
        assert!(rl.check(IP));
    }
    // Tighten to 1 q/s/client: one allowed, the immediate next denied
    // (negligible refill between back-to-back calls).
    rl.set_rate(1);
    assert!(rl.check(IP));
    assert!(!rl.check(IP));
}

#[test]
fn refills_over_time() {
    let rl = RateLimiter::new(0);
    let t = Instant::now();
    for _ in 0..3 {
        assert!(rl.check_at(IP, 3, t));
    }
    assert!(!rl.check_at(IP, 3, t));
    // One second later the bucket has fully refilled.
    let later = t + Duration::from_secs(1);
    assert!(rl.check_at(IP, 3, later));
}

#[test]
fn distinct_keys_get_independent_buckets_across_shards() {
    // With buckets sharded by key hash, distinct clients must still each get
    // their own bucket — a query must never consume a token from a different
    // client that happened to hash to the same shard.
    let rl = RateLimiter::new(0);
    let t = Instant::now();
    for i in 0..1000u32 {
        let ip = std::net::IpAddr::V4(std::net::Ipv4Addr::from(0x0A00_0100 + i));
        // Capacity 2: this client's own burst, independent of every other.
        assert!(rl.check_at(ip, 2, t), "client {i} first query");
        assert!(rl.check_at(ip, 2, t), "client {i} second query");
        assert!(!rl.check_at(ip, 2, t), "client {i} third query denied");
    }
}
