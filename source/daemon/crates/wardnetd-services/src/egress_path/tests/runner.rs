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
