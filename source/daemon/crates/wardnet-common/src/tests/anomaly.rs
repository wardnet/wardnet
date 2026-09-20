use crate::anomaly::{
    Anomaly, AnomalyQueryStatus, AnomalyReport, AnomalySeverity, AnomalyType, ReevaluateSummary,
};

fn sample(anomaly_type: AnomalyType) -> Anomaly {
    Anomaly {
        id: uuid::Uuid::nil(),
        anomaly_type,
        subject_id: None,
        message: "something happened".to_owned(),
        details: None,
        opened_at: chrono::Utc::now(),
        last_seen_at: chrono::Utc::now(),
        occurrences: 1,
        resolved_at: None,
    }
}

/// The slug is the wire form — it is the stored column, the notification
/// `kind`, and what the SDKs match on. Pin every one of them.
#[test]
fn type_slugs_are_stable() {
    assert_eq!(
        AnomalyType::TunnelStartFailed.as_str(),
        "tunnel_start_failed"
    );
    assert_eq!(AnomalyType::TunnelUnhealthy.as_str(), "tunnel_unhealthy");
    assert_eq!(AnomalyType::UpdateFailed.as_str(), "update_failed");
    assert_eq!(AnomalyType::DhcpConflict.as_str(), "dhcp_conflict");
    assert_eq!(AnomalyType::RouteTableLost.as_str(), "route_table_lost");
    assert_eq!(
        AnomalyType::BlocklistRefreshFailing.as_str(),
        "blocklist_refresh_failing"
    );
}

#[test]
fn severity_slugs_are_stable() {
    assert_eq!(AnomalySeverity::Error.as_str(), "error");
    assert_eq!(AnomalySeverity::Warning.as_str(), "warning");
    assert_eq!(AnomalySeverity::Info.as_str(), "info");
}

/// `ALL` is what the registry iterates to check every type has a detector, so
/// a variant missing from it would silently escape that check.
#[test]
fn all_contains_every_variant() {
    // Bump this alongside the enum; the count is the whole point of the test.
    assert_eq!(AnomalyType::ALL.len(), 7);

    let mut slugs: Vec<&str> = AnomalyType::ALL.iter().map(|t| t.as_str()).collect();
    slugs.sort_unstable();
    slugs.dedup();
    assert_eq!(slugs.len(), AnomalyType::ALL.len(), "slugs must be unique");
}

#[test]
fn from_slug_round_trips_every_variant() {
    for &t in AnomalyType::ALL {
        assert_eq!(AnomalyType::from_slug(t.as_str()), Some(t));
    }
}

/// A row written by a newer daemon and read after a downgrade must not blow up
/// the whole listing — callers skip unknown slugs.
#[test]
fn from_slug_rejects_unknown_slug() {
    assert_eq!(AnomalyType::from_slug("solar_flare"), None);
    assert_eq!(AnomalyType::from_slug(""), None);
}

#[test]
fn every_type_has_a_hint_component_and_url() {
    for &t in AnomalyType::ALL {
        assert!(!t.hint().is_empty(), "{} has no hint", t.as_str());
        assert!(!t.component().is_empty(), "{} has no component", t.as_str());
        assert!(
            t.url().starts_with('/'),
            "{} url must be app-relative",
            t.as_str()
        );
    }
}

/// The serde form feeds the HTTP API and both SDKs, so it has to agree with
/// `as_str` rather than merely being close to it.
#[test]
fn serde_form_matches_as_str() {
    for &t in AnomalyType::ALL {
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, format!("\"{}\"", t.as_str()));

        let back: AnomalyType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }
}

#[test]
fn query_status_defaults_to_open() {
    assert_eq!(AnomalyQueryStatus::default(), AnomalyQueryStatus::Open);
}

#[test]
fn is_open_tracks_resolved_at() {
    let mut a = sample(AnomalyType::TunnelUnhealthy);
    assert!(a.is_open());

    a.resolved_at = Some(chrono::Utc::now());
    assert!(!a.is_open());
}

#[test]
fn report_builder_sets_subject_details_and_time() {
    let at = chrono::Utc::now() - chrono::Duration::minutes(5);
    let report = AnomalyReport::new(AnomalyType::BlocklistRefreshFailing, "list is failing")
        .with_subject("blocklist-1")
        .with_details(serde_json::json!({ "profile_id": "p1" }))
        .observed_at(at);

    assert_eq!(report.subject_id.as_deref(), Some("blocklist-1"));
    assert_eq!(report.details.unwrap()["profile_id"], "p1");
    assert_eq!(report.observed_at, at);
    assert_eq!(report.message, "list is failing");
}

#[test]
fn report_defaults_have_no_subject_or_details() {
    let report = AnomalyReport::new(AnomalyType::RouteTableLost, "table 100 vanished");
    assert!(report.subject_id.is_none());
    assert!(report.details.is_none());
}

#[test]
fn reevaluate_summary_defaults_to_zero() {
    let s = ReevaluateSummary::default();
    assert_eq!(s.evaluated, 0);
    assert_eq!(s.resolved, 0);
}
