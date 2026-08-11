use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use hickory_proto::rr::RecordType;

use wardnet_common::dns::UpstreamId;

use crate::dns_filter::filter::normalize_owned;

type CacheKey = (UpstreamId, String, RecordType);

/// DNS wire-format record type for EDNS `OPT`. Its "TTL" field is actually
/// extended-rcode + version + flags, not a real TTL, so the aging walk must
/// leave it untouched.
const RR_TYPE_OPT: u16 = 41;

/// Fixed-size portion of a resource record following its NAME: TYPE(2) +
/// CLASS(2) + TTL(4) + RDLENGTH(2).
const RR_FIXED_LEN: usize = 10;

/// Byte offset of the TTL field within a record's fixed portion (past TYPE and
/// CLASS).
const RR_TTL_OFFSET: usize = 4;

/// Once the eviction queue holds this many slots for an empty-ish cache it
/// gets compacted regardless of the live-entry count, so a small cache can't
/// keep a long tail of stale slots alive.
const ORDER_COMPACT_FLOOR: usize = 64;

/// A cached DNS response with TTL-aware expiration.
struct CachedEntry {
    /// The full response in wire format, exactly as sent to the first client
    /// that populated it (transaction id included but irrelevant — the read
    /// path overwrites it). Serving a hit is a buffer copy + txid patch +
    /// in-place TTL aging, with no per-hit `Message` clone or re-encode.
    wire: Vec<u8>,
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

    /// Look up a cached response keyed by upstream pool, returning it as a
    /// send-ready wire buffer stamped with the caller's transaction `id`.
    /// Returns `None` if not found or expired. Expired entries are left for
    /// the eviction machinery to reclaim so lookups stay shared-access.
    ///
    /// The returned buffer's record TTLs are aged down by the entry's time in
    /// cache and capped at the entry's remaining lifetime, so a client hitting
    /// near the end of the entry's lifetime can't over-cache the records
    /// downstream — neither past their original TTL nor past the admin's
    /// `ttl_max` clamp. Aging happens in place on the wire bytes; no `Message`
    /// is parsed, cloned, or re-encoded on the hit path.
    pub fn get(
        &self,
        upstream: UpstreamId,
        domain: &str,
        rtype: RecordType,
        id: u16,
    ) -> Option<Vec<u8>> {
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
        let mut wire = entry.wire.clone();
        // Stamp the querying client's transaction id over the cached one — the
        // only header field a hit needs to change (2 bytes, big-endian).
        if wire.len() >= 2 {
            wire[..2].copy_from_slice(&id.to_be_bytes());
        }
        // Whole seconds for both, so a fresh hit (sub-second age) doesn't
        // round the remaining lifetime down below the original TTL.
        let elapsed = age.as_secs();
        age_wire_ttls(
            &mut wire,
            elapsed,
            entry.ttl.as_secs().saturating_sub(elapsed),
        );
        Some(wire)
    }

    /// Insert a response — as its wire-format bytes, exactly as sent — into
    /// the cache with the given TTL, keyed by the upstream pool that produced
    /// it. The send paths already hold these bytes (the built response or the
    /// raw upstream datagram), so caching costs no extra encode.
    ///
    /// The TTL is clamped between `ttl_min` and `ttl_max` seconds.
    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        &mut self,
        upstream: UpstreamId,
        domain: &str,
        rtype: RecordType,
        wire: Vec<u8>,
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
                wire,
                inserted_at: Instant::now(),
                ttl: Duration::from_secs(u64::from(ttl)),
                seq,
            },
        );
        self.compact_insertion_order();
    }

    /// Remove every cache entry at or below `domain` — the name itself plus
    /// all of its subdomains — across all upstream pools and record types.
    /// Returns the number of entries removed.
    ///
    /// The scope matches the way local DNS is *applied*: a conditional
    /// forwarding rule, an authoritative zone, and a wildcard record each
    /// govern a whole subtree, and the cache is consulted before any of them
    /// is used. Evicting only the exact name would leave every already-cached
    /// subdomain resolving the old way until its TTL expired, which for a
    /// negative answer carrying a parent zone's SOA minimum can be hours.
    ///
    /// A wildcard domain (`*.example.com`) is evicted as its suffix subtree.
    /// That also takes the apex, one name wider than the wildcard actually
    /// covers; over-eviction costs a single re-resolution, under-eviction
    /// costs correctness.
    pub fn invalidate_subtree(&mut self, domain: &str) -> u64 {
        let d = canonical_domain(domain.strip_prefix("*.").unwrap_or(domain));
        let before = self.entries.len() as u64;
        self.entries.retain(|(_, cached_domain, _), _| {
            cached_domain != &d
                && !cached_domain
                    .strip_suffix(d.as_str())
                    .is_some_and(|prefix| prefix.ends_with('.'))
        });
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

/// Age every resource record's TTL in a wire-format response by its time in
/// cache, capping at the entry's remaining lifetime so the admin's `ttl_max`
/// clamp bounds what downstream resolvers cache, not just how long this cache
/// keeps the entry. Nonzero TTLs floor at 1 so a still-valid entry never
/// serves a wrapped TTL; a TTL of 0 is an upstream do-not-cache signal and is
/// left alone.
///
/// Walks the header counts and question section to reach the records, then
/// each record's NAME (following the length/pointer encoding) to reach its
/// TTL field. `OPT` records are skipped — their "TTL" field is EDNS metadata,
/// not a real TTL. The walk is fully bounds-checked: a buffer that doesn't
/// parse cleanly (never expected — we cached bytes we ourselves sent) simply
/// stops aging where it fell off, rather than panicking on the hot path.
fn age_wire_ttls(buf: &mut [u8], elapsed_secs: u64, remaining_secs: u64) {
    let elapsed = u32::try_from(elapsed_secs).unwrap_or(u32::MAX);
    let remaining = u32::try_from(remaining_secs).unwrap_or(u32::MAX);
    let _ = age_wire_ttls_inner(buf, elapsed, remaining);
}

/// Fallible core of [`age_wire_ttls`]; returns `None` the moment an offset
/// would run past the buffer, leaving the remainder untouched.
fn age_wire_ttls_inner(buf: &mut [u8], elapsed: u32, remaining: u32) -> Option<()> {
    // Header is 12 bytes: id(2) flags(2) qdcount(2) then the three record
    // section counts — answer(2), authority(2), additional(2). All three
    // sections carry real records to age, so sum them into one record count.
    let question_count = u16::from_be_bytes([*buf.get(4)?, *buf.get(5)?]);
    let record_count = u32::from(u16::from_be_bytes([*buf.get(6)?, *buf.get(7)?]))
        + u32::from(u16::from_be_bytes([*buf.get(8)?, *buf.get(9)?]))
        + u32::from(u16::from_be_bytes([*buf.get(10)?, *buf.get(11)?]));

    let mut pos = 12;
    for _ in 0..question_count {
        pos = skip_name(buf, pos)?;
        pos = pos.checked_add(4)?; // QTYPE(2) + QCLASS(2)
    }

    for _ in 0..record_count {
        pos = skip_name(buf, pos)?;
        let rtype = u16::from_be_bytes([*buf.get(pos)?, *buf.get(pos + 1)?]);
        let ttl_at = pos.checked_add(RR_TTL_OFFSET)?;
        let rdlen_at = pos.checked_add(8)?;
        let rdlength = usize::from(u16::from_be_bytes([
            *buf.get(rdlen_at)?,
            *buf.get(rdlen_at + 1)?,
        ]));

        // OPT carries EDNS state in its TTL slot — never a real TTL.
        if rtype != RR_TYPE_OPT {
            let ttl = u32::from_be_bytes([
                *buf.get(ttl_at)?,
                *buf.get(ttl_at + 1)?,
                *buf.get(ttl_at + 2)?,
                *buf.get(ttl_at + 3)?,
            ]);
            if ttl != 0 {
                let aged = ttl.saturating_sub(elapsed).min(remaining).max(1);
                buf.get_mut(ttl_at..ttl_at + 4)?
                    .copy_from_slice(&aged.to_be_bytes());
            }
        }

        pos = pos.checked_add(RR_FIXED_LEN)?.checked_add(rdlength)?;
    }
    Some(())
}

/// Advance past a DNS NAME starting at `pos`, returning the offset of the byte
/// after it. A name is a run of length-prefixed labels ending in a zero byte,
/// or a 2-byte compression pointer (top two bits set) that terminates it. The
/// reserved `0x40`/`0x80` length forms are treated as malformed.
fn skip_name(buf: &[u8], mut pos: usize) -> Option<usize> {
    loop {
        let len = *buf.get(pos)?;
        match len & 0xC0 {
            0x00 => {
                if len == 0 {
                    return Some(pos + 1);
                }
                pos = pos.checked_add(1 + usize::from(len))?;
            }
            0xC0 => return pos.checked_add(2),
            _ => return None,
        }
    }
}

/// Canonical cache-key form of a domain: lowercase, no trailing dot. The
/// server inserts wire-format FQDNs ("foo.com.") while eviction callers pass
/// bare names ("foo.com"); both must map to the same key. Delegates to the
/// filter's owned normalizer so every DNS path agrees on canonical form —
/// the key is always owned, so the single-pass `normalize_owned` fits here.
fn canonical_domain(domain: &str) -> String {
    normalize_owned(domain)
}
