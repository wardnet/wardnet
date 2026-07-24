//! Per-client DNS rate limiting (Stage 4, #218).
//!
//! A token bucket per client key: capacity (burst) = `rate` queries, refilled
//! at `rate`/sec. `rate == 0` disables limiting. Checked at the very top of
//! query handling so a flooding client is shed before any resolution work.
//!
//! Generic over the key: the UDP pipeline keys buckets by source `IpAddr`;
//! the `DoT` listener keys them by grant token (issue #912), because its peer
//! address can be a relay's loopback and per-IP limiting there would pool
//! every roaming device into one bucket.

use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, RandomState};
use std::net::IpAddr;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

/// Number of independent bucket shards. A query locks only its key's shard,
/// so unrelated clients no longer serialize against one global lock, and the
/// at-capacity purge scans a single shard instead of the whole map — which
/// matters precisely in the spoofed-source flood the limiter exists to shed,
/// where a global purge under one lock would stall every query at once.
const SHARDS: usize = 16;

/// Upper bound on tracked client buckets across all shards. A key-spoofing
/// flood would otherwise grow the maps without limit; once a shard is full we
/// drop idle (full) buckets in that shard before admitting a new client.
const MAX_TRACKED_CLIENTS: usize = 65_536;

/// Per-shard bucket ceiling. Keys hash uniformly across shards, so the global
/// cap is `SHARDS * MAX_TRACKED_PER_SHARD` == [`MAX_TRACKED_CLIENTS`].
const MAX_TRACKED_PER_SHARD: usize = MAX_TRACKED_CLIENTS / SHARDS;

struct Bucket {
    tokens: f64,
    last: Instant,
}

/// Per-client token-bucket rate limiter, keyed by `K` (source IP for UDP,
/// grant token for `DoT`). The active rate is held in an atomic so the hot
/// path (`check`) needs no config lock; it's refreshed via
/// [`set_rate`](Self::set_rate) on config change.
///
/// Buckets are split across [`SHARDS`] independently-locked maps so a query
/// contends only with others whose key hashes to the same shard, and the
/// capacity purge is bounded to one shard's worth of keys.
pub(crate) struct RateLimiter<K = IpAddr> {
    rate: AtomicU32,
    shards: [Mutex<HashMap<K, Bucket>>; SHARDS],
    /// Per-instance randomised hasher for shard selection, so a spoofed-source
    /// flood can't deterministically pile every key onto one shard.
    shard_hasher: RandomState,
}

impl<K: Eq + Hash> RateLimiter<K> {
    pub(crate) fn new(rate: u32) -> Self {
        Self {
            rate: AtomicU32::new(rate),
            shards: std::array::from_fn(|_| Mutex::new(HashMap::new())),
            shard_hasher: RandomState::new(),
        }
    }

    /// Update the active rate (queries/sec/client; `0` disables).
    pub(crate) fn set_rate(&self, rate: u32) {
        self.rate.store(rate, Ordering::Relaxed);
    }

    /// Returns `true` if a query from `key` is allowed under the current
    /// rate (`0` disables limiting). Consumes one token on allow. Reads
    /// the rate lock-free; uses a monotonic clock for refill.
    pub(crate) fn check(&self, key: K) -> bool {
        self.check_at(key, self.rate.load(Ordering::Relaxed), Instant::now())
    }

    /// Testable core: same as [`check`](Self::check) with an injected clock.
    pub(crate) fn check_at(&self, key: K, rate: u32, now: Instant) -> bool {
        if rate == 0 {
            return true;
        }
        let rate = f64::from(rate);
        // Reduce in u64 first; the result is < SHARDS, so the cast can't
        // truncate.
        let shard = usize::try_from(self.shard_hasher.hash_one(&key) % SHARDS as u64)
            .expect("shard index < SHARDS fits usize");
        let mut map = self.shards[shard]
            .lock()
            .expect("rate-limiter mutex poisoned");

        if !map.contains_key(&key) && map.len() >= MAX_TRACKED_PER_SHARD {
            map.retain(|_, b| {
                let refilled = (b.tokens
                    + now.saturating_duration_since(b.last).as_secs_f64() * rate)
                    .min(rate);
                refilled < rate // keep only buckets that aren't fully idle
            });
        }

        let bucket = map.entry(key).or_insert(Bucket {
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
