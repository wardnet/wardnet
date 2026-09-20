//! Tests for the shared reachability handle (#1199).
//!
//! The distinction that matters throughout: an address the prober has *not
//! measured* is not the same as one it has measured and found down. Collapsing
//! the two would empty the forwarding pool on every startup and raise an
//! anomaly for every upstream on a box where DNS is switched off.

use wardnet_common::dns::UpstreamLatency;

use crate::dns::UpstreamHealth;

fn latency(address: &str, avg_latency_ms: Option<f64>, reachable: bool) -> UpstreamLatency {
    UpstreamLatency {
        address: address.to_owned(),
        avg_latency_ms,
        reachable,
    }
}

#[test]
fn a_fresh_handle_knows_nothing() {
    let health = UpstreamHealth::new();
    assert!(health.snapshot().is_empty());
    assert!(health.unreachable().is_empty());
    assert!(
        !health.is_unreachable("1.1.1.1"),
        "unmeasured is not unreachable"
    );
    assert_eq!(health.latency_ms("1.1.1.1"), None);
}

#[test]
fn publishing_replaces_the_whole_snapshot() {
    let health = UpstreamHealth::new();
    health.publish(vec![latency("1.1.1.1", Some(20.0), false)]);
    assert_eq!(health.unreachable(), vec!["1.1.1.1".to_owned()]);

    // A later round that no longer lists the address drops it entirely — the
    // snapshot is the whole truth, not an accumulating log.
    health.publish(vec![latency("8.8.8.8", Some(30.0), true)]);
    assert!(health.unreachable().is_empty());
    assert!(!health.is_unreachable("1.1.1.1"));
}

#[test]
fn unreachable_lists_only_measured_failures() {
    let health = UpstreamHealth::new();
    health.publish(vec![
        latency("1.1.1.1", Some(20.0), true),
        latency("8.8.8.8", None, false),
        latency("9.9.9.9", Some(40.0), false),
    ]);

    assert_eq!(
        health.unreachable(),
        vec!["8.8.8.8".to_owned(), "9.9.9.9".to_owned()]
    );
    assert!(health.is_unreachable("8.8.8.8"));
    assert!(!health.is_unreachable("1.1.1.1"));
}

#[test]
fn latency_is_reported_only_once_a_sample_exists() {
    let health = UpstreamHealth::new();
    health.publish(vec![
        latency("1.1.1.1", Some(20.5), true),
        // Down and never successfully probed, so there is no round-trip to
        // report — distinct from "0ms".
        latency("8.8.8.8", None, false),
    ]);

    assert_eq!(health.latency_ms("1.1.1.1"), Some(20.5));
    assert_eq!(health.latency_ms("8.8.8.8"), None);
    assert_eq!(health.latency_ms("9.9.9.9"), None);
}

#[test]
fn an_unreachable_upstream_can_still_carry_its_last_known_latency() {
    // The prober keeps the last good EWMA through a failure streak, so the UI
    // can show what the server used to manage. That must not make it look
    // reachable.
    let health = UpstreamHealth::new();
    health.publish(vec![latency("8.8.8.8", Some(35.0), false)]);

    assert!(health.is_unreachable("8.8.8.8"));
    assert_eq!(health.latency_ms("8.8.8.8"), Some(35.0));
}
