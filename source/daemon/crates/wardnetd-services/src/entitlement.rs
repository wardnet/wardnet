//! Process-wide **entitlement** state — the daemon's local view of whether its
//! wardnet subscription is active.
//!
//! Two independent axes compose into "is this box entitled to the premium app
//! surfaces right now":
//!
//! * `premium` — whether the box is on the wardnet-operated DDNS provider at
//!   all. A free/BYO-domain box (DDNS unconfigured, or the Cloudflare BYOD
//!   provider) is `premium = false` forever; it never runs the cloud
//!   token-mint flow below, so nothing else would ever clear it.
//! * `suspended` — whether the wardnet provider's last token mint was refused.
//!   Only meaningful while `premium`; a free box never mints, so this stays
//!   `false` for it regardless.
//!
//! [`is_entitled`](Entitlement::is_entitled) is `premium && !suspended`. The
//! cloud is authoritative for `suspended`: it enforces the subscription at
//! token-mint, network-register, and tunnel-connect. The daemon's only signal
//! is a `403` ("subscription is not active") when
//! [`DaemonIdentity`](crate::cloud::DaemonIdentity) mints a JWT. `premium` is
//! driven locally by the DDNS provider config (set/cleared by `enroll` +
//! `register_network`, `configure_cloudflare`, and `teardown`). Both are
//! recorded here, on a single shared handle, so the whole daemon agrees on one
//! truth:
//!
//! * the cloud clients **flip** `suspended` — suspend on a `403` mint, restore
//!   on the next successful mint;
//! * the DDNS service **flips** `premium` whenever the configured provider
//!   changes;
//! * the API/serving layer **reads** [`is_entitled`](Entitlement::is_entitled)
//!   to gate the premium app surfaces (user PWA + admin mobile app) while
//!   leaving the admin **website** reachable so the operator can always
//!   (re)subscribe;
//! * the DDNS / TLS background runners **read** [`is_suspended`](Entitlement::is_suspended)
//!   to stay inert while suspended (their calls would `403` anyway).
//!
//! Both fields are lock-free [`AtomicBool`]s behind an [`Arc`]; transitions are
//! logged once (on the edge) so the journal shows when a box's state changed,
//! not every poll.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Shared entitlement state. Construct one per process via [`Entitlement::shared`]
/// and clone the `Arc` to every reader/writer.
#[derive(Debug, Default)]
pub struct Entitlement {
    /// `true` once a token mint was refused for entitlement; cleared on the next
    /// successful mint. Starts `false` (assume active until told otherwise, so
    /// a fresh boot is never gratuitously suspended). Only meaningful while
    /// `premium`.
    suspended: AtomicBool,
    /// Whether the box is currently on the wardnet-operated DDNS provider.
    /// Starts `false` — a never-subscribed / free BYO-domain box is not
    /// entitled to the premium app surfaces by default. Flipped by the DDNS
    /// service when the configured provider changes.
    premium: AtomicBool,
}

impl Entitlement {
    /// A fresh shared handle, not premium-enrolled and not suspended.
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Whether the daemon currently believes its subscription has lapsed.
    /// Meaningless (always effectively inert) while not [`premium`](Self::is_entitled).
    #[must_use]
    pub fn is_suspended(&self) -> bool {
        self.suspended.load(Ordering::Acquire)
    }

    /// Whether this box is entitled to the premium app surfaces right now:
    /// on the wardnet provider, and not suspended.
    #[must_use]
    pub fn is_entitled(&self) -> bool {
        self.premium.load(Ordering::Acquire) && !self.is_suspended()
    }

    /// Record that a token mint was refused for entitlement. Logs once on the
    /// active → suspended edge.
    pub fn suspend(&self) {
        if self
            .suspended
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            tracing::warn!(
                "entitlement suspended: the wardnet subscription is not active; \
                 premium app surfaces are disabled until it is restored"
            );
        }
    }

    /// Record a successful token mint. Logs once on the suspended → active edge.
    pub fn restore(&self) {
        if self
            .suspended
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            tracing::info!("entitlement restored: the wardnet subscription is active again");
        }
    }

    /// Record whether the box is currently on the wardnet-operated DDNS
    /// provider. Called by the DDNS service whenever the configured provider
    /// changes (enrollment completing, switching to BYOD, or teardown), and
    /// once at startup to prime the flag from persisted config. Logs once on
    /// each edge.
    pub fn set_premium(&self, premium: bool) {
        let prev = self.premium.swap(premium, Ordering::AcqRel);
        if prev != premium {
            if premium {
                tracing::info!(
                    "entitlement premium: box is now on the wardnet DDNS provider; \
                     premium app surfaces are available (subject to suspension)"
                );
            } else {
                tracing::info!(
                    "entitlement premium: box is no longer on the wardnet DDNS provider; \
                     premium app surfaces are disabled"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Entitlement;

    #[test]
    fn starts_not_entitled() {
        let e = Entitlement::shared();
        assert!(!e.is_suspended());
        assert!(!e.is_entitled(), "a fresh handle is not premium-enrolled");
    }

    #[test]
    fn suspend_then_restore_is_idempotent() {
        let e = Entitlement::shared();
        e.suspend();
        e.suspend(); // second is a no-op edge
        assert!(e.is_suspended());
        e.restore();
        e.restore();
        assert!(!e.is_suspended());
    }

    #[test]
    fn premium_active_is_entitled() {
        let e = Entitlement::shared();
        e.set_premium(true);
        assert!(e.is_entitled());
    }

    #[test]
    fn premium_suspended_is_not_entitled() {
        let e = Entitlement::shared();
        e.set_premium(true);
        e.suspend();
        assert!(!e.is_entitled());
        e.restore();
        assert!(e.is_entitled());
    }

    #[test]
    fn free_provider_is_never_entitled_even_if_restored() {
        let e = Entitlement::shared();
        // A free/BYO box never mints, so suspend()/restore() are never called
        // on it in practice — but even if they were, `premium=false` must win.
        e.restore();
        assert!(!e.is_entitled());
    }

    #[test]
    fn set_premium_is_idempotent_and_toggles() {
        let e = Entitlement::shared();
        e.set_premium(true);
        e.set_premium(true); // no-op edge
        assert!(e.is_entitled());
        e.set_premium(false);
        assert!(!e.is_entitled());
    }
}
