//! Unit tests for [`Diagnostic`] event mapping and the [`DiagnosticStore`]
//! ring buffer.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;
use wardnet_common::event::WardnetEvent;
use wardnet_common::update::InstallPhase;

use super::listener::DiagnosticsListener;
use super::{
    Diagnostic, DiagnosticCode, DiagnosticSeverity, DiagnosticSink, DiagnosticStore,
    RecentDiagnostics,
};
use crate::event::{BroadcastEventBus, EventPublisher};

fn tunnel_start_failed() -> WardnetEvent {
    WardnetEvent::TunnelStartFailed {
        tunnel_id: Uuid::new_v4(),
        interface_name: "wg-work".to_owned(),
        error: "handshake timed out".to_owned(),
        timestamp: Utc::now(),
    }
}

#[test]
fn maps_tunnel_start_failed_to_error_diagnostic() {
    let diag = Diagnostic::from_event(&tunnel_start_failed()).expect("mapped");
    assert_eq!(diag.code, DiagnosticCode::TunnelStartFailed);
    assert_eq!(diag.severity, DiagnosticSeverity::Error);
    assert_eq!(diag.component, "tunnel");
    // The specifics from the event land in the message...
    assert!(diag.message.contains("wg-work"));
    assert!(diag.message.contains("handshake timed out"));
    // ...and the hint is the catalogue default for the code.
    assert_eq!(diag.hint, DiagnosticCode::TunnelStartFailed.hint());
}

#[test]
fn maps_dhcp_conflict_to_warning() {
    let event = WardnetEvent::DhcpConflictDetected {
        mac: "aa:bb:cc:dd:ee:ff".to_owned(),
        ip: "192.168.1.20".to_owned(),
        details: "already leased".to_owned(),
        timestamp: Utc::now(),
    };
    let diag = Diagnostic::from_event(&event).expect("mapped");
    assert_eq!(diag.code, DiagnosticCode::DhcpConflict);
    assert_eq!(diag.severity, DiagnosticSeverity::Warning);
    assert!(diag.message.contains("192.168.1.20"));
}

#[test]
fn maps_update_failed_and_route_table_lost() {
    let update = WardnetEvent::UpdateFailed {
        target_version: "2026.09.00".to_owned(),
        phase: InstallPhase::Verifying,
        error: "checksum mismatch".to_owned(),
        timestamp: Utc::now(),
    };
    let diag = Diagnostic::from_event(&update).expect("mapped");
    assert_eq!(diag.code, DiagnosticCode::UpdateFailed);
    assert!(diag.message.contains("2026.09.00"));

    let route = WardnetEvent::RouteTableLost {
        table: 51_820,
        timestamp: Utc::now(),
    };
    let diag = Diagnostic::from_event(&route).expect("mapped");
    assert_eq!(diag.code, DiagnosticCode::RouteTableLost);
    assert!(diag.message.contains("51820"));
}

#[test]
fn ignores_non_error_events() {
    let event = WardnetEvent::DnsServerStarted {
        timestamp: Utc::now(),
    };
    assert!(Diagnostic::from_event(&event).is_none());
}

#[test]
fn preserves_source_event_timestamp() {
    let event = tunnel_start_failed();
    let WardnetEvent::TunnelStartFailed { timestamp, .. } = &event else {
        unreachable!()
    };
    let expected = *timestamp;
    let diag = Diagnostic::from_event(&event).expect("mapped");
    assert_eq!(diag.timestamp, expected);
}

#[test]
fn store_starts_empty() {
    let store = DiagnosticStore::new(4);
    assert!(store.recent().is_empty());
}

#[test]
fn store_returns_entries_oldest_first() {
    let store = DiagnosticStore::new(4);
    store.record(Diagnostic::from_event(&tunnel_start_failed()).unwrap());
    store.record(
        Diagnostic::from_event(&WardnetEvent::RouteTableLost {
            table: 1,
            timestamp: Utc::now(),
        })
        .unwrap(),
    );

    let recent = store.recent();
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].code, DiagnosticCode::TunnelStartFailed);
    assert_eq!(recent[1].code, DiagnosticCode::RouteTableLost);
}

#[test]
fn store_evicts_oldest_beyond_capacity() {
    let store = DiagnosticStore::new(2);
    for _ in 0..3 {
        store.record(Diagnostic::from_event(&tunnel_start_failed()).unwrap());
    }
    // Capacity is 2, so the third insert drops the first.
    assert_eq!(store.recent().len(), 2);
}

#[test]
fn store_clones_share_one_buffer() {
    let writer = DiagnosticStore::new(4);
    let reader = writer.clone();
    writer.record(Diagnostic::from_event(&tunnel_start_failed()).unwrap());
    // The clone observes writes through the shared inner buffer — this is what
    // lets the listener (write) and log service (read) hold separate handles.
    assert_eq!(reader.recent().len(), 1);
}

#[test]
fn severity_and_code_string_forms_are_stable() {
    assert_eq!(DiagnosticSeverity::Error.as_str(), "error");
    assert_eq!(DiagnosticSeverity::Warning.as_str(), "warning");
    assert_eq!(DiagnosticSeverity::Info.as_str(), "info");
    assert_eq!(
        DiagnosticCode::TunnelStartFailed.as_str(),
        "tunnel_start_failed"
    );
    assert_eq!(DiagnosticCode::UpdateFailed.as_str(), "update_failed");
    assert_eq!(DiagnosticCode::DhcpConflict.as_str(), "dhcp_conflict");
    assert_eq!(DiagnosticCode::RouteTableLost.as_str(), "route_table_lost");
}

#[test]
fn every_code_has_a_non_empty_hint() {
    for code in [
        DiagnosticCode::TunnelStartFailed,
        DiagnosticCode::UpdateFailed,
        DiagnosticCode::DhcpConflict,
        DiagnosticCode::RouteTableLost,
    ] {
        assert!(!code.hint().is_empty(), "{code:?} must have a hint");
    }
}

/// Poll `pred` for up to ~500 ms, yielding to let the listener task run.
async fn eventually(pred: impl Fn() -> bool) -> bool {
    for _ in 0..100 {
        if pred() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    pred()
}

#[tokio::test]
async fn listener_records_error_events_and_ignores_the_rest() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let store = DiagnosticStore::new(8);
    let listener =
        DiagnosticsListener::start(&bus, Arc::new(store.clone()), &tracing::Span::current());

    // `start` subscribes synchronously before spawning, so events published now
    // are guaranteed to be seen by the task.
    bus.publish(tunnel_start_failed());
    bus.publish(WardnetEvent::DnsServerStarted {
        timestamp: Utc::now(),
    });

    assert!(
        eventually(|| store.recent().len() == 1).await,
        "the error event should be recorded and the non-error one ignored"
    );
    assert_eq!(store.recent()[0].code, DiagnosticCode::TunnelStartFailed);

    listener.shutdown().await;
}

#[tokio::test]
async fn listener_exits_when_the_event_bus_closes() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(4));
    let store = DiagnosticStore::new(4);
    let listener =
        DiagnosticsListener::start(&bus, Arc::new(store.clone()), &tracing::Span::current());

    // Dropping the only bus handle drops the broadcast sender, so the listener's
    // recv() returns Closed and the task exits on its own. The sleep lets it
    // observe the close before we cancel, exercising that branch.
    drop(bus);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // shutdown() still completes cleanly against the already-finished task.
    listener.shutdown().await;
    assert!(store.recent().is_empty());
}
