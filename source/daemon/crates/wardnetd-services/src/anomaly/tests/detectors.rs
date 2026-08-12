use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;
use wardnet_common::anomaly::{Anomaly, AnomalyStatus, AnomalyType};
use wardnet_common::tunnel::TunnelStatus;

use super::support::{DnsFilterProfileStub, FakeDnsFilter, FakeTunnels, blocklist, tunnel};
use crate::anomaly::detector::AnomalyDetector;
use crate::anomaly::detectors::{
    BlocklistRefreshFailingDetector, TransientDetector, TunnelStartFailedDetector,
    TunnelUnhealthyDetector, UpdateFailedDetector,
};

fn anomaly(anomaly_type: AnomalyType, subject: Option<&str>) -> Anomaly {
    Anomaly {
        id: Uuid::new_v4(),
        anomaly_type,
        subject_id: subject.map(str::to_owned),
        message: "something".to_owned(),
        details: None,
        opened_at: Utc::now(),
        last_seen_at: Utc::now(),
        occurrences: 1,
        resolved_at: None,
    }
}

fn with_details(mut a: Anomaly, details: serde_json::Value) -> Anomaly {
    a.details = Some(details);
    a
}

// ---------------------------------------------------------------------------
// Blocklist refresh
// ---------------------------------------------------------------------------

#[tokio::test]
async fn blocklist_detect_reports_lists_at_or_past_the_threshold() {
    let profile = Uuid::new_v4();
    let failing = Uuid::new_v4();
    let dns_filter = FakeDnsFilter::new(
        5,
        vec![DnsFilterProfileStub {
            id: profile,
            blocklists: vec![
                blocklist(failing, profile, "HaGeZi Threat Intel", 5),
                blocklist(Uuid::new_v4(), profile, "Healthy", 0),
                blocklist(Uuid::new_v4(), profile, "Nearly", 4),
            ],
        }],
    );
    let detector = BlocklistRefreshFailingDetector::new(dns_filter);

    let reports = detector.detect().await.unwrap();

    assert_eq!(reports.len(), 1);
    assert_eq!(
        reports[0].subject_id.as_deref(),
        Some(failing.to_string()).as_deref()
    );
    assert_eq!(
        reports[0].anomaly_type,
        AnomalyType::BlocklistRefreshFailing
    );
    assert!(reports[0].message.contains("HaGeZi Threat Intel"));
    assert!(reports[0].message.contains('5'));
    // `profile_id` is what makes `reevaluate` and the UI deep link possible.
    let details = reports[0].details.as_ref().unwrap();
    assert_eq!(details["profile_id"], profile.to_string());
    assert_eq!(details["consecutive_failures"], 5);
}

/// The acceptance criterion that a list which has *never* succeeded is
/// covered, not just one that was healthy and regressed. The fixture's
/// `last_updated` is `None`, which is exactly that case.
#[tokio::test]
async fn blocklist_detect_covers_lists_that_never_succeeded() {
    let profile = Uuid::new_v4();
    let never = blocklist(Uuid::new_v4(), profile, "Never worked", 9);
    assert!(never.last_updated.is_none());
    let dns_filter = FakeDnsFilter::new(
        5,
        vec![DnsFilterProfileStub {
            id: profile,
            blocklists: vec![never],
        }],
    );

    let reports = BlocklistRefreshFailingDetector::new(dns_filter)
        .detect()
        .await
        .unwrap();

    assert_eq!(reports.len(), 1);
}

#[tokio::test]
async fn blocklist_detect_is_silent_when_the_threshold_is_zero() {
    let profile = Uuid::new_v4();
    let dns_filter = FakeDnsFilter::new(
        0,
        vec![DnsFilterProfileStub {
            id: profile,
            blocklists: vec![blocklist(Uuid::new_v4(), profile, "Broken", 99)],
        }],
    );

    let reports = BlocklistRefreshFailingDetector::new(dns_filter)
        .detect()
        .await
        .unwrap();

    assert!(reports.is_empty(), "0 disables alerting entirely");
}

#[tokio::test]
async fn blocklist_detect_ignores_disabled_lists() {
    let profile = Uuid::new_v4();
    let mut disabled = blocklist(Uuid::new_v4(), profile, "Turned off", 20);
    disabled.enabled = false;
    let dns_filter = FakeDnsFilter::new(
        5,
        vec![DnsFilterProfileStub {
            id: profile,
            blocklists: vec![disabled],
        }],
    );

    let reports = BlocklistRefreshFailingDetector::new(dns_filter)
        .detect()
        .await
        .unwrap();

    assert!(reports.is_empty(), "a disabled list is not being refreshed");
}

#[tokio::test]
async fn blocklist_detector_sweeps_on_an_interval() {
    let detector = BlocklistRefreshFailingDetector::new(FakeDnsFilter::new(5, Vec::new()));
    assert!(
        detector.interval().is_some(),
        "the blocklist detector is preventive"
    );
}

/// The recovery signal: any successful refresh zeroes the counter.
#[tokio::test]
async fn blocklist_reevaluate_resolves_once_the_counter_clears() {
    let profile = Uuid::new_v4();
    let id = Uuid::new_v4();
    let dns_filter = FakeDnsFilter::new(
        5,
        vec![DnsFilterProfileStub {
            id: profile,
            blocklists: vec![blocklist(id, profile, "Recovered", 0)],
        }],
    );
    let detector = BlocklistRefreshFailingDetector::new(dns_filter);
    let a = with_details(
        anomaly(AnomalyType::BlocklistRefreshFailing, Some(&id.to_string())),
        serde_json::json!({ "profile_id": profile.to_string() }),
    );

    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

#[tokio::test]
async fn blocklist_reevaluate_stays_open_while_it_keeps_failing() {
    let profile = Uuid::new_v4();
    let id = Uuid::new_v4();
    let dns_filter = FakeDnsFilter::new(
        5,
        vec![DnsFilterProfileStub {
            id: profile,
            blocklists: vec![blocklist(id, profile, "Still broken", 12)],
        }],
    );
    let detector = BlocklistRefreshFailingDetector::new(dns_filter);
    let a = with_details(
        anomaly(AnomalyType::BlocklistRefreshFailing, Some(&id.to_string())),
        serde_json::json!({ "profile_id": profile.to_string() }),
    );

    assert_eq!(detector.reevaluate(&a).await.unwrap(), AnomalyStatus::Open);
}

#[tokio::test]
async fn blocklist_reevaluate_resolves_when_the_list_is_gone() {
    let profile = Uuid::new_v4();
    let dns_filter = FakeDnsFilter::new(
        5,
        vec![DnsFilterProfileStub {
            id: profile,
            blocklists: Vec::new(),
        }],
    );
    let detector = BlocklistRefreshFailingDetector::new(dns_filter);
    let a = with_details(
        anomaly(
            AnomalyType::BlocklistRefreshFailing,
            Some(&Uuid::new_v4().to_string()),
        ),
        serde_json::json!({ "profile_id": profile.to_string() }),
    );

    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

#[tokio::test]
async fn blocklist_reevaluate_resolves_when_the_profile_is_gone() {
    let dns_filter = FakeDnsFilter::new(5, Vec::new());
    let detector = BlocklistRefreshFailingDetector::new(dns_filter);
    let a = with_details(
        anomaly(
            AnomalyType::BlocklistRefreshFailing,
            Some(&Uuid::new_v4().to_string()),
        ),
        serde_json::json!({ "profile_id": Uuid::new_v4().to_string() }),
    );

    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

/// An anomaly with no usable subject cannot be acted on, so it is closed
/// rather than left on the dashboard forever.
#[tokio::test]
async fn blocklist_reevaluate_resolves_an_anomaly_with_no_details() {
    let detector = BlocklistRefreshFailingDetector::new(FakeDnsFilter::new(5, Vec::new()));
    let a = anomaly(AnomalyType::BlocklistRefreshFailing, Some("not-a-uuid"));

    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

// ---------------------------------------------------------------------------
// Tunnels
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tunnel_start_failed_resolves_once_the_tunnel_is_up() {
    let id = Uuid::new_v4();
    let detector =
        TunnelStartFailedDetector::new(FakeTunnels::new(vec![tunnel(id, TunnelStatus::Up)]));

    let a = anomaly(AnomalyType::TunnelStartFailed, Some(&id.to_string()));
    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

/// A bring-up that failed and was never fixed leaves the tunnel down — which
/// is still a live problem, not a resolution.
#[tokio::test]
async fn tunnel_start_failed_stays_open_while_the_tunnel_is_down() {
    let id = Uuid::new_v4();
    let detector =
        TunnelStartFailedDetector::new(FakeTunnels::new(vec![tunnel(id, TunnelStatus::Down)]));

    let a = anomaly(AnomalyType::TunnelStartFailed, Some(&id.to_string()));
    assert_eq!(detector.reevaluate(&a).await.unwrap(), AnomalyStatus::Open);
}

#[tokio::test]
async fn tunnel_start_failed_resolves_when_the_tunnel_is_deleted() {
    let detector = TunnelStartFailedDetector::new(FakeTunnels::new(Vec::new()));

    let a = anomaly(
        AnomalyType::TunnelStartFailed,
        Some(&Uuid::new_v4().to_string()),
    );
    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

#[tokio::test]
async fn tunnel_unhealthy_resolves_once_the_tunnel_is_up() {
    let id = Uuid::new_v4();
    let detector =
        TunnelUnhealthyDetector::new(FakeTunnels::new(vec![tunnel(id, TunnelStatus::Up)]));

    let a = anomaly(AnomalyType::TunnelUnhealthy, Some(&id.to_string()));
    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

/// The difference from `TunnelStartFailed`: "it should be working and isn't"
/// stops being true the moment an admin deliberately stops the tunnel.
#[tokio::test]
async fn tunnel_unhealthy_resolves_when_the_tunnel_is_deliberately_down() {
    let id = Uuid::new_v4();
    let detector =
        TunnelUnhealthyDetector::new(FakeTunnels::new(vec![tunnel(id, TunnelStatus::Down)]));

    let a = anomaly(AnomalyType::TunnelUnhealthy, Some(&id.to_string()));
    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

#[tokio::test]
async fn tunnel_unhealthy_stays_open_while_reconnecting() {
    let id = Uuid::new_v4();
    let detector = TunnelUnhealthyDetector::new(FakeTunnels::new(vec![tunnel(
        id,
        TunnelStatus::Reconnecting,
    )]));

    let a = anomaly(AnomalyType::TunnelUnhealthy, Some(&id.to_string()));
    assert_eq!(detector.reevaluate(&a).await.unwrap(), AnomalyStatus::Open);
}

#[tokio::test]
async fn tunnel_detectors_are_reactive_only() {
    let tunnels: Arc<_> = FakeTunnels::new(Vec::new());
    assert!(
        TunnelStartFailedDetector::new(tunnels.clone())
            .interval()
            .is_none()
    );
    assert!(TunnelUnhealthyDetector::new(tunnels).interval().is_none());
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_failed_resolves_once_the_box_reaches_the_target() {
    let detector = UpdateFailedDetector::new("2026.09.00");
    let a = with_details(
        anomaly(AnomalyType::UpdateFailed, None),
        serde_json::json!({ "target_version": "2026.09.00" }),
    );

    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

/// A later release overtaking the failed target also counts.
#[tokio::test]
async fn update_failed_resolves_when_a_later_release_overtook_the_target() {
    let detector = UpdateFailedDetector::new("2026.10.01");
    let a = with_details(
        anomaly(AnomalyType::UpdateFailed, None),
        serde_json::json!({ "target_version": "2026.09.00" }),
    );

    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

#[tokio::test]
async fn update_failed_stays_open_while_the_box_is_behind() {
    let detector = UpdateFailedDetector::new("2026.08.00");
    let a = with_details(
        anomaly(AnomalyType::UpdateFailed, None),
        serde_json::json!({ "target_version": "2026.09.00" }),
    );

    assert_eq!(detector.reevaluate(&a).await.unwrap(), AnomalyStatus::Open);
}

#[tokio::test]
async fn update_failed_resolves_without_a_recorded_target() {
    let detector = UpdateFailedDetector::new("2026.08.00");
    let a = anomaly(AnomalyType::UpdateFailed, None);

    assert_eq!(
        detector.reevaluate(&a).await.unwrap(),
        AnomalyStatus::Resolved
    );
}

// ---------------------------------------------------------------------------
// Transient
// ---------------------------------------------------------------------------

/// These have no authoritative check, so they declare a window instead. The
/// service applies it; `reevaluate` itself never resolves.
#[tokio::test]
async fn transient_detectors_declare_a_stale_window_and_never_self_resolve() {
    for anomaly_type in [AnomalyType::RouteTableLost, AnomalyType::DhcpConflict] {
        let detector = TransientDetector::new(anomaly_type);
        assert_eq!(detector.anomaly_type(), anomaly_type);
        assert!(detector.interval().is_none(), "nothing to sweep for");
        assert!(
            detector.stale_after().is_some(),
            "{} must declare an expiry",
            anomaly_type.as_str()
        );

        let a = anomaly(anomaly_type, None);
        assert_eq!(detector.reevaluate(&a).await.unwrap(), AnomalyStatus::Open);
    }
}

#[tokio::test]
async fn transient_stale_window_is_overridable() {
    let detector = TransientDetector::new(AnomalyType::RouteTableLost)
        .with_stale_after(Duration::from_secs(1));
    assert_eq!(detector.stale_after(), Some(Duration::from_secs(1)));
}
