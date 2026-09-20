//! Tests for the forwarding ladder's composition (#1199): which upstreams
//! serve, in what order, and how the pool survives a config that yields
//! nothing usable.
//!
//! The ladder's *behaviour* under failure — bounded deadlines, failing over,
//! exact log labels — is exercised against real sockets in `tests/server.rs`.
//! What is checked here is the pure selection logic, which is where the
//! eviction and ordering rules actually live.

use std::sync::Arc;

use wardnet_common::dns::{
    DnsConfig, DnsProtocol, ForwarderSelectionMode, UpstreamDns, UpstreamLatency,
};
use wardnetd_services::dns::UpstreamHealth;

use crate::dns::upstream_pool::{UpstreamEntry, UpstreamPool};

fn udp(address: &str, name: &str) -> UpstreamDns {
    UpstreamDns {
        address: address.to_owned(),
        name: name.to_owned(),
        protocol: DnsProtocol::Udp,
        port: None,
        tls_server_name: None,
    }
}

fn config_with(upstreams: Vec<UpstreamDns>, mode: ForwarderSelectionMode) -> DnsConfig {
    DnsConfig {
        upstream_servers: upstreams,
        forwarder_selection_mode: mode,
        single_upstream: None,
        ..DnsConfig::default()
    }
}

/// Publish a health snapshot the way a probe round would.
fn health_with(entries: &[(&str, Option<f64>, bool)]) -> Arc<UpstreamHealth> {
    let health = Arc::new(UpstreamHealth::new());
    health.publish(
        entries
            .iter()
            .map(|(address, avg_latency_ms, reachable)| UpstreamLatency {
                address: (*address).to_owned(),
                avg_latency_ms: *avg_latency_ms,
                reachable: *reachable,
            })
            .collect(),
    );
    health
}

fn addresses(entries: &[Arc<UpstreamEntry>]) -> Vec<&str> {
    entries.iter().map(|e| e.address()).collect()
}

// ---------------------------------------------------------------------------
// Composition
// ---------------------------------------------------------------------------

#[test]
fn a_fresh_pool_serves_every_configured_upstream_in_order() {
    // Reachability is not consulted at build time: a config change is a fresh
    // start, so nothing is evicted until the next probe round says so.
    let cfg = config_with(
        vec![
            udp("1.1.1.1", "CF"),
            udp("8.8.8.8", "G"),
            udp("9.9.9.9", "Q9"),
        ],
        ForwarderSelectionMode::Failover,
    );
    let pool = UpstreamPool::build(&cfg);
    assert_eq!(
        addresses(pool.serving()),
        vec!["1.1.1.1", "8.8.8.8", "9.9.9.9"]
    );
}

#[test]
fn single_mode_serves_only_the_pinned_server() {
    let cfg = DnsConfig {
        forwarder_selection_mode: ForwarderSelectionMode::Single,
        single_upstream: Some("8.8.8.8".to_owned()),
        ..config_with(
            vec![udp("1.1.1.1", "CF"), udp("8.8.8.8", "G")],
            ForwarderSelectionMode::Single,
        )
    };
    let pool = UpstreamPool::build(&cfg);
    assert_eq!(addresses(pool.serving()), vec!["8.8.8.8"]);
}

#[test]
fn an_orphaned_pin_degrades_to_the_full_pool() {
    // API validation should prevent it, but an out-of-band KV edit or a stale
    // upgrade can leave `single_upstream` naming a server that is no longer
    // configured. Serving the whole pool is the safe degradation; serving
    // nothing would fall through to the hard-coded Cloudflare backstop, which
    // is a privacy regression for an admin who never chose it.
    let cfg = DnsConfig {
        forwarder_selection_mode: ForwarderSelectionMode::Single,
        single_upstream: Some("9.9.9.9".to_owned()),
        ..config_with(
            vec![udp("1.1.1.1", "CF"), udp("8.8.8.8", "G")],
            ForwarderSelectionMode::Single,
        )
    };
    let pool = UpstreamPool::build(&cfg);
    assert_eq!(addresses(pool.serving()), vec!["1.1.1.1", "8.8.8.8"]);
}

#[test]
fn unusable_upstreams_are_dropped_and_the_rest_still_serve() {
    // A non-IP address cannot be turned into a name server (we never resolve
    // an upstream's hostname — that needs the resolver we are building), and
    // an encrypted upstream without an SNI must be dropped rather than
    // silently downgraded to plaintext.
    let cfg = config_with(
        vec![
            udp("not-an-ip", "broken"),
            UpstreamDns {
                tls_server_name: None,
                protocol: DnsProtocol::Tls,
                ..udp("8.8.8.8", "dot-no-sni")
            },
            udp("9.9.9.9", "Q9"),
        ],
        ForwarderSelectionMode::Failover,
    );
    let pool = UpstreamPool::build(&cfg);
    assert_eq!(addresses(pool.serving()), vec!["9.9.9.9"]);
}

#[test]
fn a_pool_with_nothing_usable_falls_back_rather_than_going_dark() {
    let cfg = config_with(
        vec![udp("not-an-ip", "broken")],
        ForwarderSelectionMode::Failover,
    );
    let pool = UpstreamPool::build(&cfg);
    assert_eq!(
        addresses(pool.serving()),
        vec!["1.1.1.1"],
        "a box with no usable upstream still resolves"
    );
}

#[test]
fn every_protocol_builds_a_serving_entry() {
    // Smoke over the protocol/port mapping: each arm has to produce a usable
    // entry, not be skipped.
    let cfg = config_with(
        vec![
            udp("1.1.1.1", "udp"),
            UpstreamDns {
                protocol: DnsProtocol::Tcp,
                port: Some(53),
                ..udp("8.8.8.8", "tcp")
            },
            UpstreamDns {
                protocol: DnsProtocol::Tls,
                tls_server_name: Some("dns.quad9.net".to_owned()),
                ..udp("9.9.9.9", "dot")
            },
            UpstreamDns {
                protocol: DnsProtocol::Https,
                tls_server_name: Some("cloudflare-dns.com".to_owned()),
                ..udp("1.0.0.1", "doh")
            },
        ],
        ForwarderSelectionMode::Failover,
    );
    let pool = UpstreamPool::build(&cfg);
    assert_eq!(pool.serving().len(), 4);
}

// ---------------------------------------------------------------------------
// Eviction — the fix for "reachability is computed, then discarded"
// ---------------------------------------------------------------------------

#[test]
fn an_unreachable_upstream_stops_being_served() {
    let cfg = config_with(
        vec![
            udp("1.1.1.1", "CF"),
            udp("8.8.8.8", "G"),
            udp("9.9.9.9", "Q9"),
        ],
        ForwarderSelectionMode::Failover,
    );
    let pool = UpstreamPool::build(&cfg);
    let health = health_with(&[
        ("1.1.1.1", Some(20.0), true),
        ("8.8.8.8", Some(30.0), false),
        ("9.9.9.9", Some(40.0), true),
    ]);

    let serving = pool.with_serving(&cfg, &health);
    assert_eq!(addresses(serving.serving()), vec!["1.1.1.1", "9.9.9.9"]);
}

#[test]
fn a_recovered_upstream_comes_back() {
    let cfg = config_with(
        vec![udp("1.1.1.1", "CF"), udp("8.8.8.8", "G")],
        ForwarderSelectionMode::Failover,
    );
    let pool = UpstreamPool::build(&cfg);

    let down = health_with(&[("1.1.1.1", Some(20.0), true), ("8.8.8.8", None, false)]);
    let evicted = pool.with_serving(&cfg, &down);
    assert_eq!(addresses(evicted.serving()), vec!["1.1.1.1"]);

    // Recovery is not debounced: one good probe is enough to put an upstream
    // back into rotation. Eviction is the debounced side (the prober requires
    // consecutive misses), which is the asymmetry we want — slow to condemn,
    // quick to forgive.
    let up = health_with(&[("1.1.1.1", Some(20.0), true), ("8.8.8.8", Some(25.0), true)]);
    let restored = evicted.with_serving(&cfg, &up);
    assert_eq!(addresses(restored.serving()), vec!["1.1.1.1", "8.8.8.8"]);
}

#[test]
fn an_unmeasured_upstream_is_not_treated_as_down() {
    // An address absent from the snapshot has no sample yet — at startup that
    // is every upstream. Reading "absent" as "unreachable" would empty the
    // serving set on every boot.
    let cfg = config_with(
        vec![udp("1.1.1.1", "CF"), udp("8.8.8.8", "G")],
        ForwarderSelectionMode::Failover,
    );
    let pool = UpstreamPool::build(&cfg);
    let health = Arc::new(UpstreamHealth::new());

    let serving = pool.with_serving(&cfg, &health);
    assert_eq!(addresses(serving.serving()), vec!["1.1.1.1", "8.8.8.8"]);
}

#[test]
fn all_upstreams_down_keeps_serving_them_all() {
    // Eviction must never produce an empty ladder: a pool of zero servers
    // answers nothing at all, whereas asking a possibly-dead server costs one
    // bounded attempt and might succeed if the prober is wrong.
    let cfg = config_with(
        vec![udp("1.1.1.1", "CF"), udp("8.8.8.8", "G")],
        ForwarderSelectionMode::Failover,
    );
    let pool = UpstreamPool::build(&cfg);
    let health = health_with(&[("1.1.1.1", None, false), ("8.8.8.8", None, false)]);

    let serving = pool.with_serving(&cfg, &health);
    assert_eq!(addresses(serving.serving()), vec!["1.1.1.1", "8.8.8.8"]);
}

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

#[test]
fn failover_keeps_the_admins_order() {
    let cfg = config_with(
        vec![
            udp("1.1.1.1", "CF"),
            udp("8.8.8.8", "G"),
            udp("9.9.9.9", "Q9"),
        ],
        ForwarderSelectionMode::Failover,
    );
    let pool = UpstreamPool::build(&cfg);
    // Latencies that would reorder the list under "Fastest" must not reorder
    // it here — the admin asked for a priority order and that is the whole
    // contract of the mode.
    let health = health_with(&[
        ("1.1.1.1", Some(90.0), true),
        ("8.8.8.8", Some(10.0), true),
        ("9.9.9.9", Some(50.0), true),
    ]);

    let serving = pool.with_serving(&cfg, &health);
    assert_eq!(
        addresses(serving.serving()),
        vec!["1.1.1.1", "8.8.8.8", "9.9.9.9"],
        "failover honours the configured order, not measured latency"
    );
}

#[test]
fn fastest_orders_by_measured_latency() {
    let cfg = config_with(
        vec![
            udp("1.1.1.1", "CF"),
            udp("8.8.8.8", "G"),
            udp("9.9.9.9", "Q9"),
        ],
        ForwarderSelectionMode::Fastest,
    );
    let pool = UpstreamPool::build(&cfg);
    let health = health_with(&[
        ("1.1.1.1", Some(90.0), true),
        ("8.8.8.8", Some(10.0), true),
        ("9.9.9.9", Some(50.0), true),
    ]);

    let serving = pool.with_serving(&cfg, &health);
    assert_eq!(
        addresses(serving.serving()),
        vec!["8.8.8.8", "9.9.9.9", "1.1.1.1"]
    );
}

#[test]
fn fastest_puts_unmeasured_upstreams_last_in_configured_order() {
    // Before the first probe round nothing has a sample, so an unmeasured
    // upstream must not outrank one we have actually timed — and among
    // themselves the unmeasured ones keep the admin's order rather than an
    // arbitrary one.
    let cfg = config_with(
        vec![
            udp("1.1.1.1", "unmeasured-a"),
            udp("8.8.8.8", "measured"),
            udp("9.9.9.9", "unmeasured-b"),
        ],
        ForwarderSelectionMode::Fastest,
    );
    let pool = UpstreamPool::build(&cfg);
    let health = health_with(&[("8.8.8.8", Some(70.0), true)]);

    let serving = pool.with_serving(&cfg, &health);
    assert_eq!(
        addresses(serving.serving()),
        vec!["8.8.8.8", "1.1.1.1", "9.9.9.9"]
    );
}

#[test]
fn eviction_and_ordering_compose() {
    // The fastest server is down: it must be dropped, and the rest still
    // ordered by latency rather than falling back to configured order.
    let cfg = config_with(
        vec![
            udp("1.1.1.1", "CF"),
            udp("8.8.8.8", "G"),
            udp("9.9.9.9", "Q9"),
        ],
        ForwarderSelectionMode::Fastest,
    );
    let pool = UpstreamPool::build(&cfg);
    let health = health_with(&[
        ("1.1.1.1", Some(90.0), true),
        ("8.8.8.8", Some(10.0), false),
        ("9.9.9.9", Some(50.0), true),
    ]);

    let serving = pool.with_serving(&cfg, &health);
    assert_eq!(addresses(serving.serving()), vec!["9.9.9.9", "1.1.1.1"]);
}
