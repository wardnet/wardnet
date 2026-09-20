//! Which paths get probed, and the shape of what the probe records.

use uuid::Uuid;
use wardnet_common::egress_path::ProbeStage;
use wardnet_common::tunnel::{Tunnel, TunnelStatus};

use crate::egress_path::runner::{probe_paths, stage_labels};

fn tunnel(label: &str, interface: &str, status: TunnelStatus) -> Tunnel {
    Tunnel {
        id: Uuid::new_v4(),
        label: label.to_owned(),
        country_code: "PT".to_owned(),
        provider: None,
        interface_name: interface.to_owned(),
        endpoint: "203.0.113.1:51820".to_owned(),
        status,
        last_handshake: None,
        bytes_tx: 0,
        bytes_rx: 0,
        created_at: chrono::Utc::now(),
        override_default_dns: false,
        server_selector: None,
        resolved_server_name: None,
        endpoint_resolved_at: None,
    }
}

/// Direct is not a tunnel and has no row anywhere, so it must be probed even
/// when there are no tunnels at all — including when listing them failed.
#[test]
fn direct_is_always_probed() {
    let paths = probe_paths(&[]);

    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].path, "direct");
    assert!(
        paths[0].interface.is_none(),
        "direct probes unbound, over the default route"
    );
}

/// A tunnel that is not up is already `TunnelUnhealthy`'s subject. Probing it
/// would raise a second anomaly for one fault.
#[test]
fn only_tunnels_that_are_up_are_probed() {
    let paths = probe_paths(&[
        tunnel("Lisbon", "wg_ward0", TunnelStatus::Up),
        tunnel("Berlin", "wg_ward1", TunnelStatus::Down),
        tunnel("Madrid", "wg_ward2", TunnelStatus::Connecting),
    ]);

    let labels: Vec<&str> = paths.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, vec!["Direct (WAN)", "Lisbon"]);
}

#[test]
fn an_up_tunnel_is_probed_bound_to_its_interface() {
    let paths = probe_paths(&[tunnel("Lisbon", "wg_ward0", TunnelStatus::Up)]);

    assert_eq!(paths[1].interface.as_deref(), Some("wg_ward0"));
}

/// `StatsBuffer` keys a series by the exact label string, so a key out of
/// alphabetical order silently becomes a different series rather than failing.
#[test]
fn stats_labels_are_sorted_json_objects() {
    let (result, latency) = stage_labels("direct", ProbeStage::Connect, true);

    assert_eq!(
        result,
        r#"{"outcome":"ok","path":"direct","stage":"connect"}"#
    );
    assert_eq!(latency, r#"{"path":"direct","stage":"connect"}"#);
}

#[test]
fn a_failed_stage_is_labelled_as_such() {
    let (result, _) = stage_labels("wg-1", ProbeStage::Transfer, false);

    assert_eq!(
        result,
        r#"{"outcome":"fail","path":"wg-1","stage":"transfer"}"#
    );
}

/// The invariant this subsystem is bound by, enforced at the source level
/// because Rust cannot express "does not implement this trait".
///
/// A failing path probe must be an anomaly and never an input to
/// `HealthMonitor`. The health-gated soft watchdog restarts `wardnetd`, so a
/// signal that depends on a third party — a VPN provider, or the endpoint the
/// probe contacts — would let someone else's outage restart the daemon. This is
/// the same rule ADR 0030 sets for a published app's reachability probe.
///
/// If this fails, the fix is to remove the health check, not to update the test.
#[test]
fn nothing_in_this_module_is_a_health_check() {
    let module = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/egress_path");

    // Split so this file's own assertion text is not the thing it finds.
    let forbidden = format!("impl {}", "HealthCheck");

    let sources = walk(&module);
    assert!(!sources.is_empty(), "found no sources to check");
    for entry in sources {
        let source = std::fs::read_to_string(&entry).unwrap();
        // The doc comments deliberately name the trait to explain the rule, so
        // only an actual `impl` is a violation.
        assert!(
            !source.contains(&forbidden),
            "{} implements the health-check trait: a failing path probe must \
             never be able to restart the daemon",
            entry.display()
        );
    }
}

/// Production sources only — the invariant is about what the daemon wires up,
/// and the test tree is where doubles legitimately impersonate things.
fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            found.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
    found
}

// ---------------------------------------------------------------------------
// What a round records and publishes
// ---------------------------------------------------------------------------

use std::sync::Arc;

use async_trait::async_trait;
use wardnet_common::egress_path::{PathProbeOutcome, StageOutcome};

use crate::egress_path::health::EgressPathHealth;
use crate::egress_path::prober::EgressPathProber;
use crate::egress_path::runner::{ProbePath, probe_and_publish};
use crate::stats::{Meter, StatsBuffer};

/// Returns a canned outcome per interface, and records what it was asked to
/// probe so a test can assert the binding.
struct FakeProber {
    outcome: PathProbeOutcome,
    probed: std::sync::Mutex<Vec<Option<String>>>,
}

impl FakeProber {
    fn new(outcome: PathProbeOutcome) -> Arc<Self> {
        Arc::new(Self {
            outcome,
            probed: std::sync::Mutex::new(Vec::new()),
        })
    }

    fn probed(&self) -> Vec<Option<String>> {
        self.probed.lock().unwrap().clone()
    }
}

#[async_trait]
impl EgressPathProber for FakeProber {
    async fn probe(&self, interface_name: Option<&str>) -> PathProbeOutcome {
        self.probed
            .lock()
            .unwrap()
            .push(interface_name.map(str::to_owned));
        self.outcome.clone()
    }
}

fn healthy() -> PathProbeOutcome {
    PathProbeOutcome {
        connect: StageOutcome::succeeded(12),
        transfer: Some(StageOutcome::succeeded(48)),
    }
}

fn degraded() -> PathProbeOutcome {
    PathProbeOutcome {
        connect: StageOutcome::succeeded(12),
        transfer: Some(StageOutcome::failed("timed out mid-handshake")),
    }
}

fn dead() -> PathProbeOutcome {
    PathProbeOutcome {
        connect: StageOutcome::failed("connection refused"),
        transfer: None,
    }
}

fn targets() -> Vec<ProbePath> {
    vec![
        ProbePath {
            path: "direct".to_owned(),
            label: "Direct (WAN)".to_owned(),
            interface: None,
        },
        ProbePath {
            path: "tunnel-1".to_owned(),
            label: "Lisbon".to_owned(),
            interface: Some("wg_ward0".to_owned()),
        },
    ]
}

fn meter() -> (Meter, Arc<StatsBuffer>) {
    let buffer = StatsBuffer::new();
    (Meter::new(buffer.clone()), buffer)
}

#[tokio::test]
async fn a_round_probes_every_target_with_its_own_binding() {
    let prober = FakeProber::new(healthy());
    let health = Arc::new(EgressPathHealth::new());
    let (meter, _buffer) = meter();

    probe_and_publish(
        &(prober.clone() as Arc<dyn EgressPathProber>),
        targets(),
        &health,
        &meter,
    )
    .await;

    assert_eq!(
        prober.probed(),
        vec![None, Some("wg_ward0".to_owned())],
        "direct probes unbound; a tunnel binds to its interface"
    );
}

#[tokio::test]
async fn a_round_publishes_one_entry_per_path() {
    let prober = FakeProber::new(healthy());
    let health = Arc::new(EgressPathHealth::new());
    let (meter, _buffer) = meter();

    probe_and_publish(
        &(prober as Arc<dyn EgressPathProber>),
        targets(),
        &health,
        &meter,
    )
    .await;

    let snapshot = health.snapshot();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot[0].path, "direct");
    assert_eq!(snapshot[1].label, "Lisbon");
    assert!(snapshot.iter().all(|p| !p.outcome.connect_failed()));
}

/// Each round replaces the snapshot rather than appending to it — a reader must
/// see the last round, not every round ever run.
#[tokio::test]
async fn a_later_round_replaces_the_published_snapshot() {
    let health = Arc::new(EgressPathHealth::new());
    let (meter, _buffer) = meter();

    probe_and_publish(
        &(FakeProber::new(healthy()) as Arc<dyn EgressPathProber>),
        targets(),
        &health,
        &meter,
    )
    .await;
    probe_and_publish(
        &(FakeProber::new(degraded()) as Arc<dyn EgressPathProber>),
        targets(),
        &health,
        &meter,
    )
    .await;

    let snapshot = health.snapshot();
    assert_eq!(snapshot.len(), 2, "not appended");
    assert!(
        snapshot.iter().all(|p| p.outcome.degraded()),
        "the latest round's verdict is what is published"
    );
}

#[tokio::test]
async fn a_dead_path_publishes_no_transfer_stage() {
    let health = Arc::new(EgressPathHealth::new());
    let (meter, _buffer) = meter();

    probe_and_publish(
        &(FakeProber::new(dead()) as Arc<dyn EgressPathProber>),
        targets(),
        &health,
        &meter,
    )
    .await;

    let snapshot = health.snapshot();
    assert!(snapshot[0].outcome.connect_failed());
    assert!(
        snapshot[0].outcome.transfer.is_none(),
        "nothing to transfer over, which is what keeps it from also reading as degraded"
    );
    assert!(!snapshot[0].outcome.degraded());
}

#[tokio::test]
async fn a_round_records_a_result_and_a_latency_for_each_stage() {
    let health = Arc::new(EgressPathHealth::new());
    let (meter, buffer) = meter();

    probe_and_publish(
        &(FakeProber::new(healthy()) as Arc<dyn EgressPathProber>),
        vec![targets().remove(0)],
        &health,
        &meter,
    )
    .await;

    let drained = buffer.drain();
    let metrics: Vec<&str> = drained.iter().map(|s| s.metric.as_str()).collect();
    assert!(metrics.contains(&"path.probe.result"));
    assert!(metrics.contains(&"path.probe.latency_ms"));
    assert_eq!(
        drained
            .iter()
            .filter(|s| s.metric == "path.probe.result")
            .count(),
        2,
        "one result sample per stage"
    );
}

/// A failed stage has no latency to report, so it contributes a result sample
/// and nothing else — a missing gauge is meaningfully different from a zero.
#[tokio::test]
async fn a_failed_stage_records_no_latency() {
    let health = Arc::new(EgressPathHealth::new());
    let (meter, buffer) = meter();

    probe_and_publish(
        &(FakeProber::new(dead()) as Arc<dyn EgressPathProber>),
        vec![targets().remove(0)],
        &health,
        &meter,
    )
    .await;

    let drained = buffer.drain();
    assert!(
        drained.iter().all(|s| s.metric != "path.probe.latency_ms"),
        "a connect that never completed has no latency"
    );
}

#[test]
fn an_unmeasured_path_is_absent_rather_than_reported_down() {
    let health = EgressPathHealth::new();

    assert!(health.snapshot().is_empty());
    assert!(health.get("direct").is_none());
}
