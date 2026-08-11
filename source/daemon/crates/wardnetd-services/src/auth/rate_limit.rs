//! Login rate limiting (ADR-0031 §12).
//!
//! Two independent counters, both required:
//!
//! - **Per identity** — stops an attacker grinding one known account's password
//!   from a botnet, where every request has a different source address.
//! - **Per source IP** — stops one host walking the household directory,
//!   trying a couple of passwords against each account so that no single
//!   identity counter ever trips.
//!
//! Either counter tripping refuses the attempt, because each defends against an
//! attack the other cannot see.
//!
//! # Why in memory
//!
//! State is deliberately process-local and lost on restart. Persisting it would
//! turn a lockout into something an attacker can *induce and leave behind* —
//! failed logins against the household's only admin would survive a reboot,
//! which is a denial-of-service primitive against the break-glass credential.
//! The threat model here is online guessing, which a restarting daemon
//! interrupts anyway; offline resistance is `Argon2id`'s job, not this module's.
//!
//! # Failure direction
//!
//! Refusals are `429` and **never** disclose whether the subject exists: the
//! identity counter is keyed on the submitted subject, so an unknown account
//! throttles exactly like a real one.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Failures tolerated before backoff begins.
///
/// Four is chosen to sit above realistic human error — a typo, a stale
/// password-manager entry, caps lock — and well below what online guessing
/// needs.
const FREE_ATTEMPTS: u32 = 4;

/// Backoff applied at the first refusal, doubling per subsequent failure.
const BASE_BACKOFF: Duration = Duration::from_secs(2);

/// Ceiling on the backoff. Without a cap, a household that genuinely forgot a
/// password would be locked out for days by a handful of attempts; with it, the
/// worst case is a one-minute wait per try, which still reduces an online
/// attacker to a negligible rate.
const MAX_BACKOFF: Duration = Duration::from_mins(1);

/// How long a quiet counter is kept before it is forgotten.
///
/// Also the pruning horizon: entries are swept opportunistically on write, so
/// the map cannot grow without bound from a stream of distinct subjects, which
/// is otherwise a trivially-reachable memory-exhaustion vector on a device with
/// a Raspberry Pi's RAM.
const ENTRY_TTL: Duration = Duration::from_mins(15);

/// Hard cap on tracked counters, enforced when pruning cannot free space.
///
/// A last-resort bound: an attacker sending distinct subjects faster than the
/// TTL sweeps them must not be able to grow this map indefinitely. On overflow
/// the whole map is cleared rather than evicting selectively — clearing is
/// `O(1)` to reason about, and the alternative (evicting the oldest) is exactly
/// what an attacker would drive to erase a real account's accumulated backoff.
const MAX_TRACKED: usize = 8192;

/// One counter's state.
#[derive(Debug, Clone, Copy)]
struct Attempts {
    /// Consecutive failures.
    failures: u32,
    /// When the most recent failure was recorded.
    last_failure: Instant,
}

impl Attempts {
    /// How long this counter must stay quiet before another attempt is allowed.
    fn backoff(&self) -> Duration {
        if self.failures <= FREE_ATTEMPTS {
            return Duration::ZERO;
        }
        // Saturating shift: beyond ~6 excess failures this is clamped by
        // MAX_BACKOFF anyway, and an unchecked shift would wrap to zero — i.e.
        // sustained attacks would switch the limiter *off*.
        let excess = self.failures - FREE_ATTEMPTS - 1;
        BASE_BACKOFF
            .saturating_mul(1_u32.checked_shl(excess.min(16)).unwrap_or(u32::MAX))
            .min(MAX_BACKOFF)
    }

    /// Remaining wait, or `None` when the attempt may proceed.
    fn retry_after(&self, now: Instant) -> Option<Duration> {
        let elapsed = now.saturating_duration_since(self.last_failure);
        self.backoff().checked_sub(elapsed).filter(|d| !d.is_zero())
    }
}

/// Per-identity and per-IP login backoff.
///
/// Cheap to clone-wrap in an `Arc`; all state lives behind one mutex, held only
/// for map operations and never across an `await`.
pub struct LoginRateLimiter {
    by_subject: Mutex<HashMap<String, Attempts>>,
    by_ip: Mutex<HashMap<String, Attempts>>,
}

impl Default for LoginRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginRateLimiter {
    /// Create an empty limiter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            by_subject: Mutex::new(HashMap::new()),
            by_ip: Mutex::new(HashMap::new()),
        }
    }

    /// Check both counters before a credential is verified.
    ///
    /// Returns the wait an attempt must respect, or `None` when it may proceed.
    /// The larger of the two waits wins, so satisfying one counter never
    /// bypasses the other.
    #[must_use]
    pub fn check(&self, subject: &str, client_ip: Option<&str>) -> Option<Duration> {
        let now = Instant::now();
        let subject_wait = Self::peek(&self.by_subject, &normalise(subject), now);
        let ip_wait = client_ip.and_then(|ip| Self::peek(&self.by_ip, ip, now));
        subject_wait.max(ip_wait)
    }

    /// Record a failed attempt against both counters.
    pub fn record_failure(&self, subject: &str, client_ip: Option<&str>) {
        let now = Instant::now();
        Self::bump(&self.by_subject, normalise(subject), now);
        if let Some(ip) = client_ip {
            Self::bump(&self.by_ip, ip.to_owned(), now);
        }
    }

    /// Clear both counters after a successful login.
    ///
    /// Success clears the source-IP counter too: a household behind one NAT
    /// address shares it, so leaving it standing would let one person's typos
    /// throttle everybody else.
    pub fn record_success(&self, subject: &str, client_ip: Option<&str>) {
        if let Ok(mut map) = self.by_subject.lock() {
            map.remove(&normalise(subject));
        }
        if let Some(ip) = client_ip
            && let Ok(mut map) = self.by_ip.lock()
        {
            map.remove(ip);
        }
    }

    /// Read one counter without mutating it.
    fn peek(map: &Mutex<HashMap<String, Attempts>>, key: &str, now: Instant) -> Option<Duration> {
        // A poisoned mutex must not lock everybody out of their own box, and it
        // must not silently disable the limiter either. There is no third
        // option available synchronously, so fail open and let the Argon2id
        // cost remain the floor.
        let guard = map.lock().ok()?;
        guard.get(key).and_then(|a| a.retry_after(now))
    }

    /// Increment one counter, pruning stale entries as we go.
    fn bump(map: &Mutex<HashMap<String, Attempts>>, key: String, now: Instant) {
        let Ok(mut guard) = map.lock() else {
            return;
        };

        guard.retain(|_, a| now.saturating_duration_since(a.last_failure) < ENTRY_TTL);
        if guard.len() >= MAX_TRACKED && !guard.contains_key(&key) {
            tracing::warn!(
                tracked = guard.len(),
                "login rate-limit table full, clearing: tracked={}",
                guard.len()
            );
            guard.clear();
        }

        let entry = guard.entry(key).or_insert(Attempts {
            failures: 0,
            last_failure: now,
        });
        entry.failures = entry.failures.saturating_add(1);
        entry.last_failure = now;
    }
}

/// Fold a submitted subject to its counter key.
///
/// Must match how the credential lookup normalises the subject, or `Admin` and
/// `admin` would get one counter each and the limit would be trivially doubled.
fn normalise(subject: &str) -> String {
    subject.trim().to_lowercase()
}
