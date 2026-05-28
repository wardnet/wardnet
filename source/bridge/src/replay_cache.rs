//! In-memory replay-prevention cache for signed requests.
//!
//! The bridge uses per-request Ed25519 signatures tied to a Unix timestamp
//! (`X-Wardnet-Timestamp`). A valid signature within the ±60 s window can be
//! replayed during that window unless we track which `(install_id, timestamp,
//! body_hash)` tuples we have already accepted.
//!
//! This cache closes that gap. Every successfully authenticated request stores
//! its key; a second request with identical parameters is rejected even if the
//! signature is cryptographically valid.
//!
//! # Expiry
//!
//! Entries expire after [`REPLAY_WINDOW_SECS`] seconds. A lazy sweep runs
//! on every `contains_or_insert` call so the cache does not grow unboundedly.
//! Because requests outside the ±60 s window are already rejected by the
//! timestamp check, the cache only ever needs to hold entries for at most
//! 2 × 60 = 120 seconds of traffic.

use std::collections::HashMap;
use std::sync::Mutex;

/// How long (seconds) a replay-cache entry is retained.
///
/// Set to twice the timestamp window so entries survive until the timestamp
/// window closes on both sides.
const REPLAY_WINDOW_SECS: i64 = 120;

/// Thread-safe in-memory cache keyed by `"{install_id}:{timestamp}:{body_hash}"`.
///
/// Values are Unix timestamps recording when the entry was first seen, used
/// for expiry.
pub struct ReplayCache {
    inner: Mutex<HashMap<String, i64>>,
}

impl ReplayCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Check whether `key` is already present; if not, insert it.
    ///
    /// Returns `true` if the key was already present (replay detected).
    /// Returns `false` if the key was freshly inserted (first use).
    ///
    /// Also prunes entries older than [`REPLAY_WINDOW_SECS`] on every call.
    pub fn contains_or_insert(&self, key: &str, now: i64) -> bool {
        let mut guard = self.inner.lock().expect("replay cache mutex poisoned");

        // Lazy expiry — prune stale entries on every call.
        guard.retain(|_, &mut ts| now - ts < REPLAY_WINDOW_SECS);

        if guard.contains_key(key) {
            return true;
        }

        guard.insert(key.to_string(), now);
        false
    }
}

impl Default for ReplayCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
