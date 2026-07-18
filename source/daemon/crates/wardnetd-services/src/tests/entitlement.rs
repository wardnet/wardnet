//! Tests for the process-wide entitlement state.

use crate::entitlement::Entitlement;

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