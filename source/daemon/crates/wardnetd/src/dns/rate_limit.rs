//! Per-client DNS rate limiting (Stage 4, #218).
//!
//! A token bucket per source IP: capacity (burst) = `rate` queries, refilled
//! at `rate`/sec. `rate == 0` disables limiting. Checked at the very top of
//! query handling so a flooding client is shed before any resolution work.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

/// Upper bound on tracked client buckets. A source-IP-spoofing flood would
/// otherwise grow the map without limit; once full we drop idle (full)
/// buckets before admitting a new client.
const MAX_TRACKED_CLIENTS: usize = 65_536;

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Per-source-IP token-bucket rate limiter. The active rate is held in an
/// atomic so the hot path (`check`) needs no config lock; it's refreshed
/// via [`set_rate`](Self::set_rate) on config change.
pub(crate) struct RateLimiter {
    rate: AtomicU32,
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
}

impl RateLimiter {
    pub(crate) fn new(rate: u32) -> Self {
        Self {
            rate: AtomicU32::new(rate),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Update the active rate (queries/sec/client; `0` disables).
    pub(crate) fn set_rate(&self, rate: u32) {
        self.rate.store(rate, Ordering::Relaxed);
    }

    /// Returns `true` if a query from `ip` is allowed under the current
    /// rate (`0` disables limiting). Consumes one token on allow. Reads
    /// the rate lock-free; uses a monotonic clock for refill.
    pub(crate) fn check(&self, ip: IpAddr) -> bool {
        self.check_at(ip, self.rate.load(Ordering::Relaxed), Instant::now())
    }

    /// Testable core: same as [`check`](Self::check) with an injected clock.
    fn check_at(&self, ip: IpAddr, rate: u32, now: Instant) -> bool {
        if rate == 0 {
            return true;
        }
        let rate = f64::from(rate);
        let mut map = self.buckets.lock().expect("rate-limiter mutex poisoned");

        if !map.contains_key(&ip) && map.len() >= MAX_TRACKED_CLIENTS {
            map.retain(|_, b| {
                let refilled = (b.tokens
                    + now.saturating_duration_since(b.last).as_secs_f64() * rate)
                    .min(rate);
                refilled < rate // keep only buckets that aren't fully idle
            });
        }

        let bucket = map.entry(ip).or_insert(Bucket {
            tokens: rate,
            last: now,
        });
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * rate).min(rate);
        bucket.last = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::RateLimiter;

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
}
