//! Short-lived, in-memory state for multi-step sign-in ceremonies.
//!
//! Both OAuth (`state` + PKCE verifier) and `WebAuthn` (the registration or
//! authentication challenge) issue a nonce in step one and must see it back,
//! exactly once, in step two.
//!
//! # Why this is never persisted
//!
//! A single-use nonce in a database is only single-use if the delete succeeds.
//! A row that fails to delete — a crash, a full disk, a transaction that rolls
//! back after the credential is accepted — is a replay primitive: the same
//! `state` or challenge can be presented again. Holding it in memory makes the
//! failure mode "the ceremony must be restarted", which is a nuisance rather
//! than a vulnerability. It also means a daemon restart invalidates every
//! in-flight ceremony, which is the correct direction.
//!
//! The cost is that a ceremony cannot span a restart, and that in a
//! multi-process deployment the two steps must reach the same process. Wardnet
//! is a single daemon on one box, so neither applies.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a ceremony may stay open.
///
/// Long enough for a person to complete a provider consent screen or a
/// platform authenticator prompt, short enough that an abandoned ceremony is
/// not a standing target.
pub const CEREMONY_TTL: Duration = Duration::from_secs(5 * 60);

/// Hard cap on concurrent open ceremonies.
///
/// Starting a ceremony is unauthenticated by necessity, so without a bound it
/// is a trivial memory-exhaustion vector on a Raspberry Pi. On overflow the
/// oldest entries are dropped: unlike the login rate limiter, evicting here
/// costs an honest user a restarted ceremony rather than erasing accumulated
/// security state.
const MAX_OPEN: usize = 256;

/// One open ceremony.
struct Entry<T> {
    value: T,
    created: Instant,
}

/// A single-use, expiring store keyed by an opaque nonce.
pub struct CeremonyStore<T> {
    entries: Mutex<HashMap<String, Entry<T>>>,
    ttl: Duration,
}

impl<T> Default for CeremonyStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> CeremonyStore<T> {
    /// A store with the production [`CEREMONY_TTL`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_ttl(CEREMONY_TTL)
    }

    /// A store with a custom TTL. Tests use a short one to exercise expiry
    /// without sleeping for minutes.
    #[must_use]
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Store `value` under `key`, evicting expired entries first.
    pub fn insert(&self, key: String, value: T) {
        let Ok(mut entries) = self.entries.lock() else {
            // A poisoned lock must not silently accept ceremonies it cannot
            // later validate — dropping the insert makes step two fail closed.
            return;
        };
        let now = Instant::now();
        entries.retain(|_, e| now.saturating_duration_since(e.created) < self.ttl);

        if entries.len() >= MAX_OPEN {
            // Drop the oldest so a flood cannot lock out new ceremonies
            // entirely.
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, e)| e.created)
                .map(|(k, _)| k.clone())
            {
                entries.remove(&oldest);
            }
            tracing::warn!(
                open = entries.len(),
                "ceremony store full, evicted the oldest entry"
            );
        }

        entries.insert(
            key,
            Entry {
                value,
                created: now,
            },
        );
    }

    /// Remove and return the value for `key`, if it exists and is unexpired.
    ///
    /// **Take, not get.** The removal is what makes a ceremony single-use, and
    /// it happens whether or not the caller goes on to accept the credential —
    /// so a failed second step cannot be retried with the same nonce.
    pub fn take(&self, key: &str) -> Option<T> {
        let mut entries = self.entries.lock().ok()?;
        let entry = entries.remove(key)?;
        if Instant::now().saturating_duration_since(entry.created) >= self.ttl {
            return None;
        }
        Some(entry.value)
    }

    /// Number of open ceremonies. Test-facing.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    /// Whether there are no open ceremonies.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
