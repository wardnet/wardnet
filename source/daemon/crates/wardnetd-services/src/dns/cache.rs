use std::collections::HashMap;
use std::time::{Duration, Instant};

use hickory_proto::op::Message;
use hickory_proto::rr::RecordType;

/// A cached DNS response with TTL-aware expiration.
struct CachedEntry {
    response: Message,
    inserted_at: Instant,
    ttl: Duration,
}

impl CachedEntry {
    fn is_expired(&self) -> bool {
        self.inserted_at.elapsed() >= self.ttl
    }
}

/// TTL-aware DNS response cache with LRU-style eviction.
///
/// Thread-safe via external `tokio::sync::RwLock` wrapping.
pub struct DnsCache {
    entries: HashMap<(String, RecordType), CachedEntry>,
    capacity: usize,
    hits: u64,
    misses: u64,
}

impl DnsCache {
    /// Create a new cache with the given maximum capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity),
            capacity,
            hits: 0,
            misses: 0,
        }
    }

    /// Look up a cached response. Returns `None` if not found or expired.
    pub fn get(&mut self, domain: &str, rtype: RecordType) -> Option<&Message> {
        let key = (domain.to_lowercase(), rtype);

        // Check if entry exists and is not expired.
        let expired = self.entries.get(&key).is_none_or(CachedEntry::is_expired);

        if expired {
            self.entries.remove(&key);
            self.misses += 1;
            return None;
        }

        self.hits += 1;
        self.entries.get(&key).map(|e| &e.response)
    }

    /// Insert a response into the cache with the given TTL.
    ///
    /// The TTL is clamped between `ttl_min` and `ttl_max` seconds.
    pub fn insert(
        &mut self,
        domain: &str,
        rtype: RecordType,
        response: Message,
        ttl_secs: u32,
        ttl_min: u32,
        ttl_max: u32,
    ) {
        // Clamp TTL.
        let ttl = ttl_secs.max(ttl_min).min(ttl_max);
        if ttl == 0 {
            return;
        }

        // Evict expired entries if at capacity.
        if self.entries.len() >= self.capacity {
            self.evict_expired();
        }

        // If still at capacity, evict oldest entry.
        if self.entries.len() >= self.capacity {
            self.evict_oldest();
        }

        let key = (domain.to_lowercase(), rtype);
        self.entries.insert(
            key,
            CachedEntry {
                response,
                inserted_at: Instant::now(),
                ttl: Duration::from_secs(u64::from(ttl)),
            },
        );
    }

    /// Remove all entries from the cache. Returns the number of entries cleared.
    pub fn flush(&mut self) -> u64 {
        let count = self.entries.len() as u64;
        self.entries.clear();
        count
    }

    /// Current number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cache hit rate as a fraction (0.0 to 1.0).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Total cache hits.
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Total cache misses.
    #[must_use]
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Remove all expired entries.
    fn evict_expired(&mut self) {
        self.entries.retain(|_, entry| !entry.is_expired());
    }

    /// Remove the oldest entry (by insertion time).
    fn evict_oldest(&mut self) {
        if let Some(oldest_key) = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.inserted_at)
            .map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest_key);
        }
    }
}
