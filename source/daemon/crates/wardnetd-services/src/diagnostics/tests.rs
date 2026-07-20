//! Unit tests for [`Diagnostic`] event mapping and the [`DiagnosticStore`]
//! ring buffer.

use chrono::Utc;
use uuid::Uuid;
use wardnet_common::event::WardnetEvent;
use wardnet_common::update::InstallPhase;

use super::{
    Diagnostic, DiagnosticCode, DiagnosticSeverity, DiagnosticSink, DiagnosticStore,
    RecentDiagnostics,
};

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
    assert_eq!(DiagnosticCode::DhcpConflict.as_str(), "dhcp_conflict");
}
