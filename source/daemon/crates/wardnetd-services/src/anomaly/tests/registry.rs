use std::sync::Arc;
use std::time::Duration;

use wardnet_common::anomaly::AnomalyType;

use super::support::{FakeDetector, FakeDnsFilter, FakeTunnels};
use crate::anomaly::registry::{AnomalyDetectorRegistry, DetectorDeps, EnabledDetectors};

fn deps() -> DetectorDeps {
    DetectorDeps {
        dns_filter: FakeDnsFilter::new(5, Vec::new()),
        upstream_health: std::sync::Arc::new(crate::dns::UpstreamHealth::new()),
        tunnel: FakeTunnels::new(Vec::new()),
        running_version: "2026.08.00".to_owned(),
    }
}

/// The catalogue and the registry have to stay in lockstep: a type with no
/// detector would silently never be swept and never be resolved.
#[test]
fn every_catalogue_type_has_a_detector_by_default() {
    let registry = AnomalyDetectorRegistry::new(&EnabledDetectors::new(), &deps());

    for &anomaly_type in AnomalyType::ALL {
        assert!(
            registry.get(anomaly_type).is_some(),
            "{} has no registered detector",
            anomaly_type.as_str()
        );
    }
}

#[test]
fn a_detector_can_be_disabled_by_configuration() {
    let mut enabled = EnabledDetectors::new();
    enabled.insert(
        AnomalyType::BlocklistRefreshFailing.as_str().to_owned(),
        false,
    );

    let registry = AnomalyDetectorRegistry::new(&enabled, &deps());

    assert!(registry.get(AnomalyType::BlocklistRefreshFailing).is_none());
    assert!(
        registry.get(AnomalyType::TunnelUnhealthy).is_some(),
        "disabling one must not disable the rest"
    );
}

#[test]
fn a_type_absent_from_the_flags_is_enabled() {
    let mut enabled = EnabledDetectors::new();
    enabled.insert("something_else".to_owned(), false);

    let registry = AnomalyDetectorRegistry::new(&enabled, &deps());

    assert!(registry.get(AnomalyType::TunnelStartFailed).is_some());
}

/// Only detectors that actually sweep belong on the engine's schedule.
#[test]
fn the_schedule_contains_only_detectors_with_an_interval() {
    let registry = AnomalyDetectorRegistry::new(&EnabledDetectors::new(), &deps());

    let schedule = registry.schedule();

    // The two state-polling detectors: a blocklist's failure counter and the
    // DNS prober's reachability snapshot. Everything else is reactive and has
    // nothing to schedule.
    let scheduled: Vec<AnomalyType> = schedule.iter().map(|(t, _)| *t).collect();
    assert_eq!(
        scheduled,
        vec![
            AnomalyType::BlocklistRefreshFailing,
            AnomalyType::DnsUpstreamUnreachable,
        ],
        "only the preventive detectors are scheduled, in slug order"
    );
}

#[test]
fn registering_replaces_the_detector_for_a_type() {
    let mut registry = AnomalyDetectorRegistry::empty();
    registry.register(Arc::new(FakeDetector::new(AnomalyType::RouteTableLost)));
    registry.register(Arc::new(
        FakeDetector::new(AnomalyType::RouteTableLost).with_interval(Duration::from_secs(30)),
    ));

    assert_eq!(registry.schedule().len(), 1, "the second registration wins");
}

#[test]
fn the_schedule_is_deterministically_ordered() {
    let mut registry = AnomalyDetectorRegistry::empty();
    registry.register(Arc::new(
        FakeDetector::new(AnomalyType::UpdateFailed).with_interval(Duration::from_secs(30)),
    ));
    registry.register(Arc::new(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing)
            .with_interval(Duration::from_mins(1)),
    ));
    registry.register(Arc::new(
        FakeDetector::new(AnomalyType::DhcpConflict).with_interval(Duration::from_secs(90)),
    ));

    let slugs: Vec<&str> = registry
        .schedule()
        .iter()
        .map(|(t, _)| t.as_str())
        .collect();

    assert_eq!(
        slugs,
        vec![
            "blocklist_refresh_failing",
            "dhcp_conflict",
            "update_failed"
        ]
    );
}

#[test]
fn an_empty_registry_has_nothing_to_schedule() {
    assert!(AnomalyDetectorRegistry::empty().schedule().is_empty());
}
