use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hickory_proto::op::Message;
use hickory_proto::rr::RecordType;

use wardnet_common::dns::UpstreamId;

use crate::dns_filter::filter::normalize;

type CacheKey = (UpstreamId, String, RecordType);

/// Once the eviction queue holds this many slots for an empty-ish cache it
/// gets compacted regardless of the live-entry count, so a small cache can't
/// keep a long tail of stale slots alive.
const ORDER_COMPACT_FLOOR: usize = 64;

/// A cached DNS response with TTL-aware expiration.
struct CachedEntry {
    response: Message,
    inserted_at: Instant,
    ttl: Duration,
    /// Matches this entry to its slot in `insertion_order`; an overwrite
    /// bumps the sequence, leaving the old slot stale.
    seq: u64,
}

impl CachedEntry {
    fn is_expired(&self) -> bool {
        self.inserted_at.elapsed() >= self.ttl
    }
}

/// TTL-aware DNS response cache with FIFO eviction at capacity.
///
/// Thread-safe via external `tokio::sync::RwLock` wrapping. Lookups take
/// `&self` (hit/miss counters are atomic), so concurrent per-query tasks
/// share a read lock on the hit path; only inserts and invalidation need
/// the write lock.
///
/// Keys carry an [`UpstreamId`] alongside the (domain, qtype) pair so
/// queries from devices that resolve via different upstream pools (e.g.
/// a tunneled device with `override_default_dns = true` vs a LAN device)
/// don't accidentally share cached answers — see issue #342.
///
/// Eviction is backed by `insertion_order`, a queue of `(key, seq)` slots
/// appended on every insert. Each slot is popped at most once, so making
/// room at capacity is amortized O(1) instead of a full scan of the map.
/// Slots go stale when their entry is overwritten or invalidated; they are
/// skipped on pop and compacted away once they outnumber live entries.
/// Expired entries are reclaimed by a batch sweep every
/// `capacity / EXPIRED_SWEEP_DIVISOR` at-capacity inserts, which amortizes
/// the O(n) sweep to O(1) per insert while keeping dead entries from
/// pinning capacity and starving out live ones.
pub struct DnsCache {
    entries: HashMap<CacheKey, CachedEntry>,
    insertion_order: VecDeque<(CacheKey, u64)>,
    next_seq: u64,
    capacity: usize,
    /// At-capacity inserts since the last expired-entry sweep.
    inserts_since_sweep: usize,
    hits: AtomicU64,
    misses: AtomicU64,
}

/// The expired-entry sweep runs every `capacity / EXPIRED_SWEEP_DIVISOR`
/// at-capacity inserts (at least every insert for tiny caches), bounding
/// its amortized cost to O(`EXPIRED_SWEEP_DIVISOR`) per insert.
const EXPIRED_SWEEP_DIVISOR: usize = 4;

impl DnsCache {
    /// Create a new cache with the given maximum capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity),
            insertion_order: VecDeque::with_capacity(capacity),
            next_seq: 0,
            capacity,
            inserts_since_sweep: 0,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Look up a cached response keyed by upstream pool. Returns `None`
    /// if not found or expired. Expired entries are left for the eviction
    /// machinery to reclaim so lookups stay shared-access.
    ///
    /// The returned message's record TTLs are aged down by the entry's
    /// time in cache and capped at the entry's remaining lifetime, so a
    /// client hitting near the end of the entry's lifetime can't
    /// over-cache the records downstream — neither past their original
    /// TTL nor past the admin's `ttl_max` clamp.
    pub fn get(&self, upstream: UpstreamId, domain: &str, rtype: RecordType) -> Option<Message> {
        let key = (upstream, canonical_domain(domain), rtype);

        let Some(entry) = self.entries.get(&key) else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        };

        let age = entry.inserted_at.elapsed();
        if age >= entry.ttl {
            self.misses.fetch_add(1, Ordering::Relaxed);
            return None;
        }

        self.hits.fetch_add(1, Ordering::Relaxed);
        let mut response = entry.response.clone();
        // Whole seconds for both, so a fresh hit (sub-second age) doesn't
        // round the remaining lifetime down below the original TTL.
        let elapsed = age.as_secs();
        age_record_ttls(
            &mut response,
            elapsed,
            entry.ttl.as_secs().saturating_sub(elapsed),
        );
        Some(response)
    }

    /// Insert a response into the cache with the given TTL, keyed by the
    /// upstream pool that produced it.
    ///
    /// The TTL is clamped between `ttl_min` and `ttl_max` seconds.
    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        &mut self,
        upstream: UpstreamId,
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

        let key = (upstream, canonical_domain(domain), rtype);

        // Only a brand-new key grows the map; overwrites reuse the slot.
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            self.inserts_since_sweep += 1;
            if self.inserts_since_sweep >= (self.capacity / EXPIRED_SWEEP_DIVISOR).max(1) {
                self.inserts_since_sweep = 0;
                self.evict_expired();
            }
            self.evict_oldest();
        }

        let seq = self.next_seq;
        self.next_seq += 1;
        self.insertion_order.push_back((key.clone(), seq));
        self.entries.insert(
            key,
            CachedEntry {
                response,
                inserted_at: Instant::now(),
                ttl: Duration::from_secs(u64::from(ttl)),
                seq,
            },
        );
        self.compact_insertion_order();
    }

    /// Remove all cache entries for `domain` across all upstream pools and
    /// record types. Returns the number of entries removed.
    pub fn invalidate_domain(&mut self, domain: &str) -> u64 {
        let d = canonical_domain(domain);
        let before = self.entries.len() as u64;
        self.entries
            .retain(|(_, cached_domain, _), _| cached_domain != &d);
        // A mass invalidation can orphan many queue slots at once; reclaim
        // them here rather than leaving the next insert to wade through
        // them under the hot-path write lock.
        self.compact_insertion_order();
        before - self.entries.len() as u64
    }

    /// Remove all entries from the cache. Returns the number of entries cleared.
    pub fn flush(&mut self) -> u64 {
        let count = self.entries.len() as u64;
        self.entries.clear();
        self.insertion_order.clear();
        count
    }

    /// Current number of entries, including expired ones not yet reclaimed
    /// by the batch sweep.
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
        let hits = self.hits();
        let total = hits + self.misses();
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Total cache hits.
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Total cache misses.
    #[must_use]
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// Batch-remove every expired entry, then the queue slots orphaned by
    /// the removals. O(n), but run at most once per
    /// `capacity / EXPIRED_SWEEP_DIVISOR` at-capacity inserts, so the cost
    /// amortizes to O(1) per insert. Without it, FIFO eviction would keep
    /// removing the oldest *live* entry while newer already-expired ones
    /// sat on capacity.
    fn evict_expired(&mut self) {
        self.entries.retain(|_, entry| !entry.is_expired());
        let entries = &self.entries;
        self.insertion_order
            .retain(|(key, seq)| entries.get(key).is_some_and(|e| e.seq == *seq));
    }

    /// Evict entries oldest-first until there is room for one more. Stale
    /// slots — whose entry was overwritten (sequence mismatch) or already
    /// removed by invalidation or the expired sweep — are popped and
    /// discarded without touching a live entry.
    fn evict_oldest(&mut self) {
        while self.entries.len() >= self.capacity {
            let Some((key, seq)) = self.insertion_order.pop_front() else {
                break;
            };
            if self.entries.get(&key).is_some_and(|e| e.seq == seq) {
                self.entries.remove(&key);
            }
        }
    }

    /// Drop stale queue slots once they outnumber live entries. Removals
    /// that bypass the queue (invalidation, the expired sweep, overwrites)
    /// leave stale slots behind, so each compaction pays for the removals
    /// that preceded it — amortized O(1) per mutation.
    fn compact_insertion_order(&mut self) {
        let threshold = self
            .entries
            .len()
            .saturating_mul(2)
            .max(ORDER_COMPACT_FLOOR);
        if self.insertion_order.len() < threshold {
            return;
        }
        let entries = &self.entries;
        self.insertion_order
            .retain(|(key, seq)| entries.get(key).is_some_and(|e| e.seq == *seq));
    }

    /// Test hook: shift an entry's insertion time into the past to simulate
    /// time spent in cache without sleeping.
    #[cfg(test)]
    pub(crate) fn backdate(
        &mut self,
        upstream: UpstreamId,
        domain: &str,
        rtype: RecordType,
        by: Duration,
    ) {
        let key = (upstream, canonical_domain(domain), rtype);
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.inserted_at -= by;
        }
    }

    /// Test hook: current number of eviction-queue slots, live and stale.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn insertion_order_len(&self) -> usize {
        self.insertion_order.len()
    }
}

/// Age a cached response's record TTLs by its time in cache, and cap them
/// at the entry's remaining lifetime so the admin's `ttl_max` clamp bounds
/// what downstream resolvers cache, not just how long this cache keeps the
/// entry. Nonzero TTLs floor at 1 so a still-valid entry never serves a
/// wrapped TTL; a TTL of 0 is an upstream do-not-cache signal and is left
/// alone. All three record sections are aged — the forwarding paths cache
/// the full upstream message, which can carry glue records in additionals
/// (OPT never appears there; hickory parses it into `Message::edns`).
fn age_record_ttls(response: &mut Message, elapsed_secs: u64, remaining_secs: u64) {
    let elapsed = u32::try_from(elapsed_secs).unwrap_or(u32::MAX);
    let remaining = u32::try_from(remaining_secs).unwrap_or(u32::MAX);
    for record in response
        .answers
        .iter_mut()
        .chain(response.authorities.iter_mut())
        .chain(response.additionals.iter_mut())
    {
        if record.ttl == 0 {
            continue;
        }
        record.ttl = record.ttl.saturating_sub(elapsed).min(remaining).max(1);
    }
}

/// Canonical cache-key form of a domain: lowercase, no trailing dot. The
/// server inserts wire-format FQDNs ("foo.com.") while eviction callers pass
/// bare names ("foo.com"); both must map to the same key. Delegates to the
/// filter's normalizer so every DNS path agrees on canonical form.
fn canonical_domain(domain: &str) -> String {
    normalize(domain)
}
