//! Process-wide **entitlement** state — the daemon's local view of whether its
//! wardnet subscription is active.
//!
//! The cloud is authoritative: it enforces the subscription at token-mint,
//! network-register, and tunnel-connect. The daemon's only signal is a `403`
//! ("subscription is not active") when [`DaemonIdentity`](crate::cloud::DaemonIdentity)
//! mints a JWT. That signal is recorded here, on a single shared handle, so the
//! whole daemon agrees on one truth:
//!
//! * the cloud clients **flip** it — suspend on a `403` mint, restore on the next
//!   successful mint;
//! * the API/serving layer **reads** it to gate the premium app surfaces (user
//!   PWA + admin mobile app) while leaving the admin **website** reachable so the
//!   operator can always resubscribe;
//! * the DDNS / TLS background runners **read** it to stay inert while suspended
//!   (their calls would `403` anyway).
//!
//! It is a lock-free [`AtomicBool`] behind an [`Arc`]; transitions are logged once
//! (on the edge) so the journal shows when a box entered or left the suspended
//! state, not every poll.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Shared entitlement state. Construct one per process via [`Entitlement::shared`]
/// and clone the `Arc` to every reader/writer.
#[derive(Debug, Default)]
pub struct Entitlement {
    /// `true` once a token mint was refused for entitlement; cleared on the next
    /// successful mint. Starts `false` (assume entitled until told otherwise, so
    /// a fresh boot is never gratuitously suspended).
    suspended: AtomicBool,
}

impl Entitlement {
    /// A fresh shared handle in the **active** (not suspended) state.
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Whether the daemon currently believes its subscription has lapsed.
    #[must_use]
    pub fn is_suspended(&self) -> bool {
        self.suspended.load(Ordering::Acquire)
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
}

#[cfg(test)]
mod tests {
    use super::Entitlement;

    #[test]
    fn starts_active() {
        assert!(!Entitlement::shared().is_suspended());
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
}
