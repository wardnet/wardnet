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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use chrono::Utc;
use wardnet_common::event::WardnetEvent;

use crate::event::EventPublisher;

/// Shared entitlement state. Construct one per process via [`Entitlement::shared`]
/// and clone the `Arc` to every reader/writer.
#[derive(Default)]
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
    /// Optional event sink, wired once after construction (the DDNS service
    /// creates this handle before the event bus exists). When present, every
    /// `is_entitled()` edge publishes [`WardnetEvent::EntitlementChanged`], so
    /// listeners can tear down Premium-only runtime state the moment the box
    /// loses entitlement (e.g. disable the inbound-WireGuard server).
    publisher: OnceLock<Arc<dyn EventPublisher>>,
}

impl std::fmt::Debug for Entitlement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `dyn EventPublisher` is not `Debug`; report the observable state only.
        f.debug_struct("Entitlement")
            .field("premium", &self.premium.load(Ordering::Relaxed))
            .field("suspended", &self.suspended.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Entitlement {
    /// A fresh shared handle, not premium-enrolled and not suspended.
    #[must_use]
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Wire the event bus so `is_entitled()` edges publish
    /// [`WardnetEvent::EntitlementChanged`]. Idempotent: the first call wins,
    /// later calls are ignored. Called once during service init, after the
    /// bus is built. Until set, edges are silent (fine for tests / the mock).
    pub fn set_publisher(&self, publisher: Arc<dyn EventPublisher>) {
        let _ = self.publisher.set(publisher);
    }

    /// Publish an [`WardnetEvent::EntitlementChanged`] iff `is_entitled()`
    /// actually flipped relative to `was_entitled`. No-op when no publisher is
    /// wired. Best-effort: the event is an optimization over the reconcile /
    /// request-time gates, and listeners re-check state, so a missed or
    /// spurious edge is not load-bearing.
    fn emit_if_entitlement_changed(&self, was_entitled: bool) {
        let now_entitled = self.is_entitled();
        if now_entitled == was_entitled {
            return;
        }
        if let Some(publisher) = self.publisher.get() {
            publisher.publish(WardnetEvent::EntitlementChanged {
                entitled: now_entitled,
                timestamp: Utc::now(),
            });
        }
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
        let was_entitled = self.is_entitled();
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
        self.emit_if_entitlement_changed(was_entitled);
    }

    /// Record a successful token mint. Logs once on the suspended → active edge.
    pub fn restore(&self) {
        let was_entitled = self.is_entitled();
        if self
            .suspended
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            tracing::info!("entitlement restored: the wardnet subscription is active again");
        }
        self.emit_if_entitlement_changed(was_entitled);
    }

    /// Record whether the box is currently on the wardnet-operated DDNS
    /// provider. Called by the DDNS service whenever the configured provider
    /// changes (enrollment completing, switching to BYOD, or teardown), and
    /// once at startup to prime the flag from persisted config. Logs once on
    /// each edge.
    pub fn set_premium(&self, premium: bool) {
        let was_entitled = self.is_entitled();
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
        self.emit_if_entitlement_changed(was_entitled);
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

    /// Records published events so edge emission can be asserted.
    #[derive(Default)]
    struct RecordingPublisher {
        events: std::sync::Mutex<Vec<wardnet_common::event::WardnetEvent>>,
    }

    impl crate::event::EventPublisher for RecordingPublisher {
        fn publish(&self, event: wardnet_common::event::WardnetEvent) {
            self.events.lock().unwrap().push(event);
        }
        fn subscribe(
            &self,
        ) -> tokio::sync::broadcast::Receiver<wardnet_common::event::WardnetEvent> {
            tokio::sync::broadcast::channel(16).1
        }
    }

    fn entitled_edges(pubr: &RecordingPublisher) -> Vec<bool> {
        pubr.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|ev| match ev {
                wardnet_common::event::WardnetEvent::EntitlementChanged { entitled, .. } => {
                    Some(*entitled)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn premium_edges_emit_entitlement_changed() {
        let e = Entitlement::shared();
        let pubr = std::sync::Arc::new(RecordingPublisher::default());
        e.set_publisher(pubr.clone());
        e.set_premium(true); // not-entitled -> entitled
        e.set_premium(true); // no-op edge, no event
        e.set_premium(false); // entitled -> not-entitled
        assert_eq!(entitled_edges(&pubr), vec![true, false]);
    }

    #[test]
    fn suspend_restore_edges_emit_only_while_premium() {
        let e = Entitlement::shared();
        let pubr = std::sync::Arc::new(RecordingPublisher::default());
        e.set_publisher(pubr.clone());
        // A free box (never premium): suspend/restore never cross the entitled
        // edge, so nothing is published.
        e.suspend();
        e.restore();
        assert!(entitled_edges(&pubr).is_empty());
        // Once premium, suspension crosses the edge and restoration crosses back.
        e.set_premium(true); // -> entitled
        e.suspend(); // -> not entitled
        e.restore(); // -> entitled
        assert_eq!(entitled_edges(&pubr), vec![true, false, true]);
    }

    #[test]
    fn no_publisher_wired_is_silent() {
        // The mock and unit tests may never wire a publisher; edges must not panic.
        let e = Entitlement::shared();
        e.set_premium(true);
        e.suspend();
        e.restore();
        e.set_premium(false);
        assert!(!e.is_entitled());
    }
}
