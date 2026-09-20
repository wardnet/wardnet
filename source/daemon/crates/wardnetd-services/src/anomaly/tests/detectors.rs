use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;
use wardnet_common::anomaly::{Anomaly, AnomalyStatus, AnomalyType};
use wardnet_common::tunnel::TunnelStatus;

use super::support::{DnsFilterProfileStub, FakeDnsFilter, FakeTunnels, blocklist, tunnel};
use crate::anomaly::detector::AnomalyDetector;
use crate::anomaly::detectors::{
    BlocklistRefreshFailingDetector, DnsUpstreamUnreachableDetector, TransientDetector,
    TunnelStartFailedDetector, TunnelUnhealthyDetector, UpdateFailedDetector,
};
use crate::dns::UpstreamHealth;

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

/// `Down` must NOT resolve this anomaly.
///
/// It reads like "an admin stopped it, so the problem is moot", but `Tunnel`
/// has no desired-state field: a deliberate tear-down and a broken tunnel are
/// indistinguishable by status. Worse, the path that *opens* this anomaly sets
/// `Down` first — `reconcile_iface_presence` marks the tunnel down before
/// publishing `TunnelDown{interface absent}` — so resolving on `Down` closed
/// the anomaly on the next pass and pushed "Problem resolved" while the
/// interface was still gone. The same happened to every open anomaly at boot,
/// because shutdown records all tunnels `Down` before the routing reconcile.
#[tokio::test]
async fn tunnel_unhealthy_stays_open_when_the_tunnel_is_down() {
    let id = Uuid::new_v4();
    let detector =
        TunnelUnhealthyDetector::new(FakeTunnels::new(vec![tunnel(id, TunnelStatus::Down)]));

    let a = anomaly(AnomalyType::TunnelUnhealthy, Some(&id.to_string()));
    assert_eq!(detector.reevaluate(&a).await.unwrap(), AnomalyStatus::Open);
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

/// Pre-release suffixes do not sort lexicographically: `"…edge.9"` compares
/// above `"…edge.10"`. A raw string compare therefore treated a failed update
/// to `edge.10` as already-applied, discarded the alert, and pushed a bogus
/// "Problem resolved". Edge and beta builds are real (`build-daemon.yml` sets
/// `WARDNET_RELEASE_VERSION_OVERRIDE` to exactly these shapes), so this is the
/// common case on a test box, not a corner.
#[tokio::test]
async fn update_failed_stays_open_across_a_prerelease_rollover() {
    let detector = UpdateFailedDetector::new("2026.08.00-edge.9");
    let a = with_details(
        anomaly(AnomalyType::UpdateFailed, None),
        serde_json::json!({ "target_version": "2026.08.00-edge.10" }),
    );

    assert_eq!(detector.reevaluate(&a).await.unwrap(), AnomalyStatus::Open);
}

/// A final release is newer than any pre-release of the same base, so a box
/// that reached `2026.08.00` has caught up with a failed `-beta.5` attempt.
#[tokio::test]
async fn update_failed_resolves_when_a_release_supersedes_a_prerelease_target() {
    let detector = UpdateFailedDetector::new("2026.08.00");
    let a = with_details(
        anomaly(AnomalyType::UpdateFailed, None),
        serde_json::json!({ "target_version": "2026.08.00-beta.5" }),
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

// ---------------------------------------------------------------------------
// DnsUpstreamUnreachableDetector (#1199)
//
// The prober already maintains reachability as state, so this detector only
// has to read it. What matters is that it reports one anomaly per failing
// upstream and — the part that keeps the dashboard honest — closes them again
// on every route back to "not currently unreachable".
// ---------------------------------------------------------------------------

fn health(entries: &[(&str, bool)]) -> Arc<UpstreamHealth> {
    let health = Arc::new(UpstreamHealth::new());
    health.publish(
        entries
            .iter()
            .map(
                |(address, reachable)| wardnet_common::dns::UpstreamLatency {
                    address: (*address).to_owned(),
                    avg_latency_ms: Some(12.0),
                    reachable: *reachable,
                },
            )
            .collect(),
    );
    health
}

#[tokio::test]
async fn reports_nothing_while_every_upstream_answers() {
    let detector =
        DnsUpstreamUnreachableDetector::new(health(&[("1.1.1.1", true), ("8.8.8.8", true)]));
    assert!(detector.detect().await.unwrap().is_empty());
}

#[tokio::test]
async fn reports_nothing_when_nothing_has_been_measured() {
    // An empty snapshot means the prober has not run, or the forwarding path
    // is not serving. Neither is evidence that an upstream is down, and
    // raising an anomaly from it would alert on every startup.
    let detector = DnsUpstreamUnreachableDetector::new(Arc::new(UpstreamHealth::new()));
    assert!(detector.detect().await.unwrap().is_empty());
}

#[tokio::test]
async fn reports_one_anomaly_per_failing_upstream() {
    let detector = DnsUpstreamUnreachableDetector::new(health(&[
        ("1.1.1.1", true),
        ("8.8.8.8", false),
        ("9.9.9.9", false),
    ]));

    let reports = detector.detect().await.unwrap();
    let subjects: Vec<Option<String>> = reports.iter().map(|r| r.subject_id.clone()).collect();
    assert_eq!(
        subjects,
        vec![Some("8.8.8.8".to_owned()), Some("9.9.9.9".to_owned())],
        "each failing upstream gets its own anomaly, keyed by address"
    );
    assert!(
        reports[0].message.contains("8.8.8.8"),
        "the message names the server: {}",
        reports[0].message
    );
}

#[tokio::test]
async fn an_open_anomaly_stays_open_while_the_upstream_is_still_down() {
    let detector = DnsUpstreamUnreachableDetector::new(health(&[("8.8.8.8", false)]));
    let status = detector
        .reevaluate(&anomaly(
            AnomalyType::DnsUpstreamUnreachable,
            Some("8.8.8.8"),
        ))
        .await
        .unwrap();
    assert_eq!(status, AnomalyStatus::Open);
}

#[tokio::test]
async fn recovery_resolves_the_anomaly() {
    let detector = DnsUpstreamUnreachableDetector::new(health(&[("8.8.8.8", true)]));
    let status = detector
        .reevaluate(&anomaly(
            AnomalyType::DnsUpstreamUnreachable,
            Some("8.8.8.8"),
        ))
        .await
        .unwrap();
    assert_eq!(status, AnomalyStatus::Resolved);
}

#[tokio::test]
async fn an_upstream_that_left_the_snapshot_resolves() {
    // Removed from the config, or DNS switched off/recursive so the prober
    // publishes nothing. The condition that opened the anomaly no longer
    // holds either way, and leaving it open would strand an entry on the
    // dashboard about a server the box no longer uses.
    for snapshot in [
        health(&[("1.1.1.1", true)]),
        Arc::new(UpstreamHealth::new()),
    ] {
        let detector = DnsUpstreamUnreachableDetector::new(snapshot);
        let status = detector
            .reevaluate(&anomaly(
                AnomalyType::DnsUpstreamUnreachable,
                Some("8.8.8.8"),
            ))
            .await
            .unwrap();
        assert_eq!(status, AnomalyStatus::Resolved);
    }
}

#[tokio::test]
async fn a_subjectless_anomaly_resolves_rather_than_lingering() {
    let detector = DnsUpstreamUnreachableDetector::new(health(&[("8.8.8.8", false)]));
    let status = detector
        .reevaluate(&anomaly(AnomalyType::DnsUpstreamUnreachable, None))
        .await
        .unwrap();
    assert_eq!(status, AnomalyStatus::Resolved);
}
