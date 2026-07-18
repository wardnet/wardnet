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
