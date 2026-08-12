use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;
use wardnet_common::anomaly::{
    AnomalyFilter, AnomalyQueryStatus, AnomalyReport, AnomalyStatus, AnomalyType,
};
use wardnetd_data::repository::AnomalyRepository;
use wardnetd_data::repository::sqlite::{SqliteAnomalyRepository, format_ts};

use super::support::{FakeDetector, RecordingPush, as_admin, test_pool};
use crate::anomaly::registry::AnomalyDetectorRegistry;
use crate::anomaly::service::{AnomalyService, AnomalyServiceImpl};
use crate::error::AppError;

struct Harness {
    service: AnomalyServiceImpl,
    repo: Arc<SqliteAnomalyRepository>,
    push: Arc<RecordingPush>,
}

async fn harness_with(
    detector: FakeDetector,
    push: Arc<RecordingPush>,
) -> (Harness, Arc<FakeDetector>) {
    let detector = Arc::new(detector);
    let mut registry = AnomalyDetectorRegistry::empty();
    registry.register(detector.clone());
    let repo = Arc::new(SqliteAnomalyRepository::new(test_pool().await));
    let service = AnomalyServiceImpl::new(repo.clone(), Arc::new(registry), push.clone())
        // Keep the timeout tests quick.
        .with_detector_timeout(Duration::from_millis(50));
    (
        Harness {
            service,
            repo,
            push,
        },
        detector,
    )
}

async fn harness() -> Harness {
    harness_with(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing),
        RecordingPush::new(),
    )
    .await
    .0
}

fn report(subject: &str) -> AnomalyReport {
    AnomalyReport::new(AnomalyType::BlocklistRefreshFailing, "list is failing")
        .with_subject(subject)
}

// ---------------------------------------------------------------------------
// submit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn submit_opens_an_anomaly_and_alerts_once() {
    let h = harness().await;

    as_admin(h.service.submit(report("bl-1"))).await.unwrap();

    let open = h.repo.list_open().await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(h.push.opened_ids(), vec![open[0].id]);
}

/// The heart of issue #1097. A broken feed is observed on every sweep for
/// days; the admin must hear about it exactly once.
#[tokio::test]
async fn repeated_submits_never_alert_again() {
    let h = harness().await;

    for _ in 0..25 {
        as_admin(h.service.submit(report("bl-1"))).await.unwrap();
    }

    assert_eq!(h.repo.list_open().await.unwrap().len(), 1);
    assert_eq!(
        h.push.opened_ids().len(),
        1,
        "continued failures must not re-alert"
    );
}

#[tokio::test]
async fn repeated_submits_refresh_the_existing_anomaly() {
    let h = harness().await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();

    as_admin(
        h.service.submit(
            AnomalyReport::new(
                AnomalyType::BlocklistRefreshFailing,
                "list is failing (12 times)",
            )
            .with_subject("bl-1")
            .with_details(serde_json::json!({ "consecutive_failures": 12 })),
        ),
    )
    .await
    .unwrap();

    let open = h.repo.list_open().await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].occurrences, 2);
    assert_eq!(open[0].message, "list is failing (12 times)");
    assert_eq!(
        open[0].details.as_ref().unwrap()["consecutive_failures"],
        12
    );
}

#[tokio::test]
async fn different_subjects_get_their_own_anomalies_and_alerts() {
    let h = harness().await;

    as_admin(h.service.submit(report("bl-1"))).await.unwrap();
    as_admin(h.service.submit(report("bl-2"))).await.unwrap();

    assert_eq!(h.repo.list_open().await.unwrap().len(), 2);
    assert_eq!(h.push.opened_ids().len(), 2);
}

#[tokio::test]
async fn submit_requires_an_admin_context() {
    let h = harness().await;

    let err = h.service.submit(report("bl-1")).await.unwrap_err();

    assert!(matches!(
        err,
        AppError::Unauthorized(_) | AppError::Forbidden(_)
    ));
}

// ---------------------------------------------------------------------------
// resolve
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolving_a_notified_anomaly_sends_one_recovery_notice() {
    let h = harness().await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();
    let id = h.repo.list_open().await.unwrap()[0].id;

    as_admin(h.service.resolve(id)).await.unwrap();

    assert!(h.repo.list_open().await.unwrap().is_empty());
    assert_eq!(h.push.resolved_ids(), vec![id]);
}

#[tokio::test]
async fn resolving_twice_sends_only_one_recovery_notice() {
    let h = harness().await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();
    let id = h.repo.list_open().await.unwrap()[0].id;

    as_admin(h.service.resolve(id)).await.unwrap();
    as_admin(h.service.resolve(id)).await.unwrap();

    assert_eq!(h.push.resolved_ids().len(), 1);
}

/// "It is working again" must never arrive without its "it is broken". If the
/// open alert failed to deliver, `notified_at` is unset and the recovery
/// notice is suppressed.
#[tokio::test]
async fn recovery_is_suppressed_when_the_open_was_never_notified() {
    let (h, _) = harness_with(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing),
        RecordingPush::failing(),
    )
    .await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();
    let id = h.repo.list_open().await.unwrap()[0].id;
    assert!(!h.repo.was_notified(id).await.unwrap());

    as_admin(h.service.resolve(id)).await.unwrap();

    assert!(h.push.resolved_ids().is_empty());
}

#[tokio::test]
async fn resolving_an_unknown_anomaly_is_a_not_found() {
    let h = harness().await;

    let err = as_admin(h.service.resolve(Uuid::new_v4()))
        .await
        .unwrap_err();

    assert!(matches!(err, AppError::NotFound(_)));
}

/// A condition that comes back after recovering is a genuinely new problem and
/// alerts again.
#[tokio::test]
async fn a_condition_that_recurs_alerts_again() {
    let h = harness().await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();
    let first = h.repo.list_open().await.unwrap()[0].id;
    as_admin(h.service.resolve(first)).await.unwrap();

    as_admin(h.service.submit(report("bl-1"))).await.unwrap();

    assert_eq!(h.push.opened_ids().len(), 2);
    assert_ne!(h.push.opened_ids()[1], first);
}

// ---------------------------------------------------------------------------
// run_detector
// ---------------------------------------------------------------------------

#[tokio::test]
async fn run_detector_submits_everything_the_sweep_reports() {
    let (h, _) = harness_with(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing)
            .with_reports(vec![report("bl-1"), report("bl-2")]),
        RecordingPush::new(),
    )
    .await;

    as_admin(h.service.run_detector(AnomalyType::BlocklistRefreshFailing))
        .await
        .unwrap();

    assert_eq!(h.repo.list_open().await.unwrap().len(), 2);
}

#[tokio::test]
async fn run_detector_is_a_not_found_for_an_unregistered_type() {
    let h = harness().await;

    let err = as_admin(h.service.run_detector(AnomalyType::RouteTableLost))
        .await
        .unwrap_err();

    assert!(matches!(err, AppError::NotFound(_)));
}

/// A detector that explodes must not fail the cycle for the others.
#[tokio::test]
async fn a_failing_detector_sweep_is_swallowed() {
    let (h, _) = harness_with(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing).failing(),
        RecordingPush::new(),
    )
    .await;

    as_admin(h.service.run_detector(AnomalyType::BlocklistRefreshFailing))
        .await
        .unwrap();

    assert!(h.repo.list_open().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_hanging_detector_sweep_is_bounded_by_the_timeout() {
    let (h, detector) = harness_with(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing).hanging(),
        RecordingPush::new(),
    )
    .await;

    as_admin(h.service.run_detector(AnomalyType::BlocklistRefreshFailing))
        .await
        .unwrap();

    assert_eq!(detector.detect_count(), 1);
    assert!(h.repo.list_open().await.unwrap().is_empty());
}

// ---------------------------------------------------------------------------
// reevaluate_all
// ---------------------------------------------------------------------------

#[tokio::test]
async fn reevaluate_resolves_anomalies_whose_condition_ended() {
    let (h, _) = harness_with(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing)
            .with_verdict(AnomalyStatus::Resolved),
        RecordingPush::new(),
    )
    .await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();

    let summary = as_admin(h.service.reevaluate_all()).await.unwrap();

    assert_eq!(summary.evaluated, 1);
    assert_eq!(summary.resolved, 1);
    assert!(h.repo.list_open().await.unwrap().is_empty());
    assert_eq!(h.push.resolved_ids().len(), 1);
}

#[tokio::test]
async fn reevaluate_leaves_anomalies_whose_condition_still_holds() {
    let (h, _) = harness_with(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing).with_verdict(AnomalyStatus::Open),
        RecordingPush::new(),
    )
    .await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();

    let summary = as_admin(h.service.reevaluate_all()).await.unwrap();

    assert_eq!(summary.resolved, 0);
    assert_eq!(h.repo.list_open().await.unwrap().len(), 1);
}

/// Silently closing a problem we failed to check would be the worst available
/// outcome, so a detector error leaves the anomaly open.
#[tokio::test]
async fn a_failing_reevaluate_leaves_the_anomaly_open() {
    let push = RecordingPush::new();
    let (h, _) = harness_with(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing),
        push.clone(),
    )
    .await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();

    // Swap in a failing detector for the reevaluation pass.
    let mut registry = AnomalyDetectorRegistry::empty();
    registry.register(Arc::new(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing).failing(),
    ));
    let service = AnomalyServiceImpl::new(h.repo.clone(), Arc::new(registry), push)
        .with_detector_timeout(Duration::from_millis(50));

    let summary = as_admin(service.reevaluate_all()).await.unwrap();

    assert_eq!(summary.resolved, 0);
    assert_eq!(h.repo.list_open().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_hanging_reevaluate_leaves_the_anomaly_open() {
    let push = RecordingPush::new();
    let (h, _) = harness_with(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing),
        push.clone(),
    )
    .await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();

    let mut registry = AnomalyDetectorRegistry::empty();
    registry.register(Arc::new(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing).hanging(),
    ));
    let service = AnomalyServiceImpl::new(h.repo.clone(), Arc::new(registry), push)
        .with_detector_timeout(Duration::from_millis(50));

    let summary = as_admin(service.reevaluate_all()).await.unwrap();

    assert_eq!(summary.resolved, 0);
    assert_eq!(h.repo.list_open().await.unwrap().len(), 1);
}

/// A detector with no authoritative check declares a window instead; the
/// service applies it before ever calling `reevaluate`.
#[tokio::test]
async fn a_stale_anomaly_is_resolved_without_consulting_the_detector() {
    let (h, _) = harness_with(
        FakeDetector::new(AnomalyType::BlocklistRefreshFailing)
            .with_stale_after(Duration::from_mins(1))
            // Would keep it open if it were ever asked.
            .with_verdict(AnomalyStatus::Open),
        RecordingPush::new(),
    )
    .await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();

    // Backdate the last sighting past the window.
    let id = h.repo.list_open().await.unwrap()[0].id;
    h.repo
        .touch(
            AnomalyType::BlocklistRefreshFailing.as_str(),
            Some("bl-1"),
            &format_ts(Utc::now() - chrono::Duration::hours(2)),
            "list is failing",
            None,
        )
        .await
        .unwrap();

    let summary = as_admin(h.service.reevaluate_all()).await.unwrap();

    assert_eq!(summary.resolved, 1);
    assert!(
        h.repo
            .find_by_id(id)
            .await
            .unwrap()
            .unwrap()
            .resolved_at
            .is_some()
    );
}

/// A disabled detector's anomalies are left alone, not force-closed by
/// something that no longer understands them.
#[tokio::test]
async fn anomalies_of_an_unregistered_type_are_left_open() {
    let h = harness().await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();

    let service = AnomalyServiceImpl::new(
        h.repo.clone(),
        Arc::new(AnomalyDetectorRegistry::empty()),
        h.push.clone(),
    );
    let summary = as_admin(service.reevaluate_all()).await.unwrap();

    assert_eq!(summary.evaluated, 1);
    assert_eq!(summary.resolved, 0);
}

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_filters_by_subject_for_per_entity_surfaces() {
    let h = harness().await;
    as_admin(h.service.submit(report("bl-1"))).await.unwrap();
    as_admin(h.service.submit(report("bl-2"))).await.unwrap();

    let mine = as_admin(h.service.list(
        AnomalyFilter {
            status: AnomalyQueryStatus::Open,
            subject_id: Some("bl-2".to_owned()),
        },
        50,
    ))
    .await
    .unwrap();

    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].subject_id.as_deref(), Some("bl-2"));
}

#[tokio::test]
async fn list_clamps_an_absurd_limit() {
    let h = harness().await;

    let all = as_admin(h.service.list(AnomalyFilter::default(), u32::MAX))
        .await
        .unwrap();

    assert!(all.is_empty());
}

#[tokio::test]
async fn list_requires_an_admin_context() {
    let h = harness().await;

    let err = h
        .service
        .list(AnomalyFilter::default(), 10)
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        AppError::Unauthorized(_) | AppError::Forbidden(_)
    ));
}
