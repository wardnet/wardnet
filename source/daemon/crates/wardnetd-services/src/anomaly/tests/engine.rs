use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::anomaly::{
    Anomaly, AnomalyFilter, AnomalyReport, AnomalyType, ReevaluateSummary,
};

use crate::anomaly::engine::AnomaliesDetectionEngine;
use crate::anomaly::service::AnomalyService;
use crate::auth_context;
use crate::error::AppError;

/// Records what the engine asked for, and asserts the calls arrive under an
/// admin context the way every other runner's do.
#[derive(Default)]
struct RecordingService {
    schedule: Vec<(AnomalyType, Duration)>,
    detector_runs: StdMutex<Vec<AnomalyType>>,
    reevaluations: StdMutex<u32>,
    saw_non_admin: StdMutex<bool>,
}

impl RecordingService {
    fn with_schedule(schedule: Vec<(AnomalyType, Duration)>) -> Arc<Self> {
        Arc::new(Self {
            schedule,
            ..Self::default()
        })
    }

    fn note_context(&self) {
        if auth_context::require_admin().is_err() {
            *self.saw_non_admin.lock().unwrap() = true;
        }
    }

    fn runs(&self) -> Vec<AnomalyType> {
        self.detector_runs.lock().unwrap().clone()
    }

    fn reevaluation_count(&self) -> u32 {
        *self.reevaluations.lock().unwrap()
    }
}

#[async_trait]
impl AnomalyService for RecordingService {
    async fn list(&self, _f: AnomalyFilter, _l: u32) -> Result<Vec<Anomaly>, AppError> {
        unimplemented!()
    }
    async fn submit(&self, _r: AnomalyReport) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn resolve(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn run_detector(&self, anomaly_type: AnomalyType) -> Result<(), AppError> {
        self.note_context();
        self.detector_runs.lock().unwrap().push(anomaly_type);
        Ok(())
    }
    async fn reevaluate_all(&self) -> Result<ReevaluateSummary, AppError> {
        self.note_context();
        *self.reevaluations.lock().unwrap() += 1;
        Ok(ReevaluateSummary::default())
    }
    fn schedule(&self) -> Vec<(AnomalyType, Duration)> {
        self.schedule.clone()
    }
}

/// A box that just booted should learn what is wrong with it without waiting
/// out a full interval first.
#[tokio::test(start_paused = true)]
async fn every_detector_runs_immediately_on_startup() {
    let service = RecordingService::with_schedule(vec![
        (AnomalyType::BlocklistRefreshFailing, Duration::from_mins(5)),
        (AnomalyType::UpdateFailed, Duration::from_hours(1)),
    ]);

    let engine = AnomaliesDetectionEngine::start(service.clone(), &tracing::Span::current());
    tokio::time::sleep(Duration::from_millis(10)).await;
    engine.shutdown().await;

    let mut runs = service.runs();
    runs.sort_by_key(|t| t.as_str());
    assert_eq!(
        runs,
        vec![
            AnomalyType::BlocklistRefreshFailing,
            AnomalyType::UpdateFailed
        ]
    );
    assert_eq!(service.reevaluation_count(), 1);
}

/// Each detector keeps its own cadence rather than sharing a global tick.
#[tokio::test(start_paused = true)]
async fn each_detector_runs_on_its_own_interval() {
    let service = RecordingService::with_schedule(vec![
        (AnomalyType::BlocklistRefreshFailing, Duration::from_mins(5)),
        (AnomalyType::UpdateFailed, Duration::from_hours(1)),
    ]);

    let engine = AnomaliesDetectionEngine::start_with_intervals(
        service.clone(),
        Duration::from_hours(24),
        &tracing::Span::current(),
    );
    // Half an hour: six sweeps of the 5-minute detector (the startup one plus
    // five more), and only the startup sweep of the hourly one.
    tokio::time::sleep(Duration::from_mins(30)).await;
    engine.shutdown().await;

    let runs = service.runs();
    let fast = runs
        .iter()
        .filter(|t| **t == AnomalyType::BlocklistRefreshFailing)
        .count();
    let slow = runs
        .iter()
        .filter(|t| **t == AnomalyType::UpdateFailed)
        .count();

    assert!(fast >= 6, "expected repeated fast sweeps, got {fast}");
    assert_eq!(slow, 1, "the hourly detector must not follow the fast one");
}

#[tokio::test(start_paused = true)]
async fn open_anomalies_are_reevaluated_on_their_own_cadence() {
    let service = RecordingService::with_schedule(Vec::new());

    let engine = AnomaliesDetectionEngine::start_with_intervals(
        service.clone(),
        Duration::from_mins(1),
        &tracing::Span::current(),
    );
    tokio::time::sleep(Duration::from_mins(5)).await;
    engine.shutdown().await;

    assert!(
        service.reevaluation_count() >= 5,
        "expected a reevaluation per minute, got {}",
        service.reevaluation_count()
    );
}

/// Reactive-only deployments still need the reevaluation pass, so the engine
/// must run with an empty schedule rather than idling or spinning.
#[tokio::test(start_paused = true)]
async fn the_engine_runs_with_no_preventive_detectors() {
    let service = RecordingService::with_schedule(Vec::new());

    let engine = AnomaliesDetectionEngine::start_with_intervals(
        service.clone(),
        Duration::from_mins(1),
        &tracing::Span::current(),
    );
    tokio::time::sleep(Duration::from_millis(10)).await;
    engine.shutdown().await;

    assert!(service.runs().is_empty());
    assert_eq!(service.reevaluation_count(), 1);
}

#[tokio::test(start_paused = true)]
async fn service_calls_run_under_an_admin_context() {
    let service = RecordingService::with_schedule(vec![(
        AnomalyType::BlocklistRefreshFailing,
        Duration::from_mins(5),
    )]);

    let engine = AnomaliesDetectionEngine::start(service.clone(), &tracing::Span::current());
    tokio::time::sleep(Duration::from_millis(10)).await;
    engine.shutdown().await;

    assert!(
        !*service.saw_non_admin.lock().unwrap(),
        "the engine must call the service under an admin context"
    );
}

#[tokio::test(start_paused = true)]
async fn shutdown_stops_the_engine() {
    let service = RecordingService::with_schedule(Vec::new());

    let engine = AnomaliesDetectionEngine::start_with_intervals(
        service.clone(),
        Duration::from_mins(1),
        &tracing::Span::current(),
    );
    tokio::time::sleep(Duration::from_millis(10)).await;
    engine.shutdown().await;
    let after_shutdown = service.reevaluation_count();

    tokio::time::sleep(Duration::from_mins(10)).await;

    assert_eq!(
        service.reevaluation_count(),
        after_shutdown,
        "no work may happen after shutdown"
    );
}
