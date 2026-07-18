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
