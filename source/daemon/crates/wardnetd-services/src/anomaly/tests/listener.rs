use chrono::{Duration as ChronoDuration, Utc};
use uuid::Uuid;
use wardnet_common::anomaly::AnomalyType;
use wardnet_common::event::{TUNNEL_DOWN_INTERFACE_ABSENT, WardnetEvent};
use wardnet_common::update::InstallPhase;

use crate::anomaly::listener::report_from_event;

#[test]
fn tunnel_start_failed_maps_to_an_anomaly_keyed_on_the_tunnel() {
    let tunnel_id = Uuid::new_v4();
    let at = Utc::now();

    let report = report_from_event(&WardnetEvent::TunnelStartFailed {
        tunnel_id,
        interface_name: "wg_ward0".to_owned(),
        error: "failed to resolve peer endpoint".to_owned(),
        timestamp: at,
    })
    .unwrap();

    assert_eq!(report.anomaly_type, AnomalyType::TunnelStartFailed);
    // The subject is half the dedup key — without it every tunnel's failures
    // would collapse into one anomaly.
    assert_eq!(
        report.subject_id.as_deref(),
        Some(tunnel_id.to_string()).as_deref()
    );
    assert!(report.message.contains("wg_ward0"));
    assert!(report.message.contains("failed to resolve peer endpoint"));
    // The anomaly carries when the condition happened, not when we noticed.
    assert_eq!(report.observed_at, at);
}

/// The handshake-timeout case #225 names. There is no error object on this
/// path, so the message is synthesized from how long it has been quiet.
#[test]
fn tunnel_reconnecting_maps_to_unhealthy_with_a_synthesized_message() {
    let tunnel_id = Uuid::new_v4();
    let at = Utc::now();

    let report = report_from_event(&WardnetEvent::TunnelReconnecting {
        tunnel_id,
        interface_name: "wg_ward1".to_owned(),
        last_handshake: Some(at - ChronoDuration::minutes(7)),
        timestamp: at,
    })
    .unwrap();

    assert_eq!(report.anomaly_type, AnomalyType::TunnelUnhealthy);
    assert_eq!(
        report.subject_id.as_deref(),
        Some(tunnel_id.to_string()).as_deref()
    );
    assert!(
        report.message.contains("7 minutes"),
        "message was {:?}",
        report.message
    );
}

#[test]
fn tunnel_reconnecting_without_a_handshake_still_reports() {
    let report = report_from_event(&WardnetEvent::TunnelReconnecting {
        tunnel_id: Uuid::new_v4(),
        interface_name: "wg_ward1".to_owned(),
        last_handshake: None,
        timestamp: Utc::now(),
    })
    .unwrap();

    assert!(report.message.contains("no handshake yet"));
}

#[test]
fn a_vanished_interface_maps_to_unhealthy() {
    let tunnel_id = Uuid::new_v4();

    let report = report_from_event(&WardnetEvent::TunnelDown {
        tunnel_id,
        interface_name: "wg_ward0".to_owned(),
        reason: TUNNEL_DOWN_INTERFACE_ABSENT.to_owned(),
        timestamp: Utc::now(),
    })
    .unwrap();

    assert_eq!(report.anomaly_type, AnomalyType::TunnelUnhealthy);
    assert_eq!(
        report.subject_id.as_deref(),
        Some(tunnel_id.to_string()).as_deref()
    );
}

/// Every other tear-down is deliberate and emphatically not an anomaly.
#[test]
fn a_deliberate_tear_down_is_not_an_anomaly() {
    for reason in ["admin requested", "tunnel deleted", "idle"] {
        assert!(
            report_from_event(&WardnetEvent::TunnelDown {
                tunnel_id: Uuid::new_v4(),
                interface_name: "wg_ward0".to_owned(),
                reason: reason.to_owned(),
                timestamp: Utc::now(),
            })
            .is_none(),
            "reason {reason:?} must not raise an anomaly"
        );
    }
}

#[test]
fn update_failed_records_the_target_version_for_reevaluation() {
    let report = report_from_event(&WardnetEvent::UpdateFailed {
        target_version: "2026.09.00".to_owned(),
        phase: InstallPhase::Verifying,
        error: "connection reset".to_owned(),
        timestamp: Utc::now(),
    })
    .unwrap();

    assert_eq!(report.anomaly_type, AnomalyType::UpdateFailed);
    assert_eq!(
        report.details.as_ref().unwrap()["target_version"],
        "2026.09.00"
    );
    // Box-wide: there is no per-subject entity for an update.
    assert!(report.subject_id.is_none());
}

#[test]
fn dhcp_conflict_maps_to_an_anomaly_keyed_on_the_address() {
    let report = report_from_event(&WardnetEvent::DhcpConflictDetected {
        mac: "aa:bb:cc:dd:ee:ff".to_owned(),
        ip: "192.168.1.50".to_owned(),
        details: "two replies".to_owned(),
        timestamp: Utc::now(),
    })
    .unwrap();

    assert_eq!(report.anomaly_type, AnomalyType::DhcpConflict);
    assert_eq!(report.subject_id.as_deref(), Some("192.168.1.50"));
}

#[test]
fn route_table_lost_maps_to_an_anomaly_keyed_on_the_table() {
    let report = report_from_event(&WardnetEvent::RouteTableLost {
        table: 100,
        timestamp: Utc::now(),
    })
    .unwrap();

    assert_eq!(report.anomaly_type, AnomalyType::RouteTableLost);
    assert_eq!(report.subject_id.as_deref(), Some("100"));
}

/// The vast majority of the bus is not an admin-actionable problem.
#[test]
fn ordinary_events_are_ignored() {
    let ignored = [
        WardnetEvent::DnsServerStarted {
            timestamp: Utc::now(),
        },
        WardnetEvent::TunnelUp {
            tunnel_id: Uuid::new_v4(),
            interface_name: "wg_ward0".to_owned(),
            endpoint: "example.test:51820".to_owned(),
            timestamp: Utc::now(),
        },
    ];

    for event in ignored {
        assert!(report_from_event(&event).is_none());
    }
}

// ---------------------------------------------------------------------------
// The running listener
// ---------------------------------------------------------------------------

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use async_trait::async_trait;
use wardnet_common::anomaly::{Anomaly, AnomalyFilter, AnomalyReport, ReevaluateSummary};

use crate::anomaly::listener::AnomalyListener;
use crate::anomaly::service::AnomalyService;
use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};

#[derive(Default)]
struct RecordingService {
    submitted: StdMutex<Vec<AnomalyType>>,
}

impl RecordingService {
    fn submitted(&self) -> Vec<AnomalyType> {
        self.submitted.lock().unwrap().clone()
    }
}

#[async_trait]
impl AnomalyService for RecordingService {
    async fn list(&self, _f: AnomalyFilter, _l: u32) -> Result<Vec<Anomaly>, AppError> {
        unimplemented!()
    }
    async fn submit(&self, report: AnomalyReport) -> Result<(), AppError> {
        self.submitted.lock().unwrap().push(report.anomaly_type);
        Ok(())
    }
    async fn resolve(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn run_detector(&self, _t: AnomalyType) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn reevaluate_all(&self) -> Result<ReevaluateSummary, AppError> {
        unimplemented!()
    }
    fn schedule(&self) -> Vec<(AnomalyType, Duration)> {
        Vec::new()
    }
}

/// Wait for the listener to drain, without pinning a sleep long enough to be
/// flaky on a loaded machine.
async fn settle(service: &Arc<RecordingService>, expected: usize) {
    for _ in 0..200 {
        if service.submitted().len() >= expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[tokio::test]
async fn the_listener_submits_error_events_and_ignores_the_rest() {
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let service = Arc::new(RecordingService::default());
    let listener = AnomalyListener::start(&events, service.clone(), &tracing::Span::current());

    events.publish(WardnetEvent::TunnelStartFailed {
        tunnel_id: Uuid::new_v4(),
        interface_name: "wg_ward0".to_owned(),
        error: "boom".to_owned(),
        timestamp: Utc::now(),
    });
    events.publish(WardnetEvent::DnsServerStarted {
        timestamp: Utc::now(),
    });

    settle(&service, 1).await;
    listener.shutdown().await;

    assert_eq!(service.submitted(), vec![AnomalyType::TunnelStartFailed]);
}

#[tokio::test]
async fn the_listener_exits_when_the_event_bus_closes() {
    let bus = Arc::new(BroadcastEventBus::new(16));
    let events: Arc<dyn EventPublisher> = bus.clone();
    let service = Arc::new(RecordingService::default());
    let listener = AnomalyListener::start(&events, service, &tracing::Span::current());

    drop(events);
    drop(bus);

    // Completes because the loop breaks on `Closed`, not because it is
    // cancelled — a listener that ignored the close would hang here.
    tokio::time::timeout(Duration::from_secs(5), listener.shutdown())
        .await
        .expect("listener should exit when the bus closes");
}
