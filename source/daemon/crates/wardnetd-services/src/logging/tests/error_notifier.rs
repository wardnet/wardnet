//! Tests for [`ErrorNotifierService`] ring buffer and its tracing layer.

use std::sync::Arc;

use tracing_subscriber::layer::SubscriberExt;

use crate::logging::component::LogComponent;
use crate::logging::error_notifier::{ErrorEntry, ErrorNotifier, ErrorNotifierService};

/// Build a configured-and-started notifier service.
fn started_service(capacity: usize) -> Arc<ErrorNotifierService> {
    let svc = Arc::new(ErrorNotifierService::new(capacity));
    svc.start();
    svc
}

// ---------------------------------------------------------------------------
// ErrorNotifierService unit tests (public API only)
// ---------------------------------------------------------------------------

#[test]
fn new_service_has_empty_buffer() {
    let svc = ErrorNotifierService::new(15);
    assert!(svc.get_recent_errors().is_empty());
}

#[test]
fn start_marks_service_active() {
    let svc = ErrorNotifierService::new(15);
    assert!(!svc.is_active());
    svc.start();
    assert!(svc.is_active());
}

#[test]
fn stop_marks_service_inactive() {
    let svc = ErrorNotifierService::new(15);
    svc.start();
    svc.stop();
    assert!(!svc.is_active());
}

// ---------------------------------------------------------------------------
// Tracing layer integration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn layer_captures_error_events() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!(target: "test", "something went wrong");

    let entries = svc.get_recent_errors();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].level, "ERROR");
    assert_eq!(entries[0].target, "test");
    assert!(
        entries[0].message.contains("something went wrong"),
        "message was: {}",
        entries[0].message
    );
}

#[tokio::test]
async fn layer_captures_warn_events() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::warn!(target: "warn_target", "a warning");

    let entries = svc.get_recent_errors();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].level, "WARN");
    assert_eq!(entries[0].target, "warn_target");
}

#[tokio::test]
async fn layer_ignores_info_events() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info!("just info");

    assert!(
        svc.get_recent_errors().is_empty(),
        "INFO should not be captured"
    );
}

#[tokio::test]
async fn layer_ignores_debug_events() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::debug!("debug details");

    assert!(
        svc.get_recent_errors().is_empty(),
        "DEBUG should not be captured"
    );
}

#[tokio::test]
async fn layer_ignores_trace_events() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::trace!("trace details");

    assert!(
        svc.get_recent_errors().is_empty(),
        "TRACE should not be captured"
    );
}

#[tokio::test]
async fn layer_captures_both_warn_and_error() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::warn!("first warning");
    tracing::error!("first error");
    tracing::warn!("second warning");

    let entries = svc.get_recent_errors();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].level, "WARN");
    assert_eq!(entries[1].level, "ERROR");
    assert_eq!(entries[2].level, "WARN");
}

#[tokio::test]
async fn layer_populates_timestamp() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!("ts check");

    let entries = svc.get_recent_errors();
    assert_eq!(entries.len(), 1);
    // Timestamp is a DateTime<Utc> -- use it to reason about ordering rather than string format.
    let ts = entries[0].timestamp;
    let now = chrono::Utc::now();
    assert!(ts <= now, "timestamp should be <= now");
}

#[tokio::test]
async fn buffer_evicts_oldest_when_at_capacity() {
    // Use the same capacity as the previous default (15) for continuity.
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    for i in 0..20 {
        tracing::error!("error {}", i);
    }

    let entries = svc.get_recent_errors();
    // Should only have the most recent 15.
    assert_eq!(entries.len(), 15);
    // The oldest entry should be error 5 (0..4 evicted).
    assert!(
        entries[0].message.contains("error 5"),
        "oldest entry message was: {}",
        entries[0].message
    );
    // The newest should be error 19.
    assert!(
        entries[14].message.contains("error 19"),
        "newest entry message was: {}",
        entries[14].message
    );
}

#[tokio::test]
async fn layer_captures_message_via_record_str() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    // Using a string literal message exercises record_str on the "message" field.
    tracing::error!("string literal message");

    let entries = svc.get_recent_errors();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].message.contains("string literal message"));
}

#[tokio::test]
async fn error_entry_fields_are_populated() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!(target: "wardnetd::api", "handler failed");

    let entries = svc.get_recent_errors();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.level, "ERROR");
    assert_eq!(entry.target, "wardnetd::api");
    assert!(entry.message.contains("handler failed"));
}

#[test]
fn error_entry_serializes_to_json() {
    let entry = ErrorEntry {
        timestamp: chrono::DateTime::parse_from_rfc3339("2026-04-13T00:00:00.000Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
        level: "ERROR".to_owned(),
        target: "wardnetd".to_owned(),
        message: "test error".to_owned(),
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["level"], "ERROR");
    assert_eq!(json["message"], "test error");
    assert_eq!(json["target"], "wardnetd");
    // Timestamp is serialized as RFC3339 string by chrono's Serialize impl.
    assert!(json["timestamp"].as_str().unwrap().contains("2026-04-13"));
}

#[test]
fn error_entry_debug_impl() {
    let entry = ErrorEntry {
        timestamp: chrono::Utc::now(),
        level: "WARN".to_owned(),
        target: "t".to_owned(),
        message: "msg".to_owned(),
    };
    let debug = format!("{entry:?}");
    assert!(debug.contains("ErrorEntry"));
}

#[tokio::test]
async fn entries_ordered_oldest_to_newest() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!("first");
    tracing::error!("second");
    tracing::error!("third");

    let entries = svc.get_recent_errors();
    assert_eq!(entries.len(), 3);
    assert!(entries[0].message.contains("first"));
    assert!(entries[1].message.contains("second"));
    assert!(entries[2].message.contains("third"));
}

#[tokio::test]
async fn mixed_levels_only_captures_warn_and_error() {
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::trace!("trace");
    tracing::debug!("debug");
    tracing::info!("info");
    tracing::warn!("warn");
    tracing::error!("error");

    let entries = svc.get_recent_errors();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].level, "WARN");
    assert_eq!(entries[1].level, "ERROR");
}

#[tokio::test]
async fn inactive_service_ignores_events() {
    // Service is never started; events should not be captured.
    let svc = Arc::new(ErrorNotifierService::new(15));
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!("should not be captured");
    assert!(svc.get_recent_errors().is_empty());
}

#[derive(Debug)]
struct DebuggableError;

#[tokio::test]
async fn layer_records_non_message_fields_via_debug_visitor() {
    // A structured field with Debug formatting (not str) on a non-"message"
    // name exercises the `record_debug` path for the field-name != "message"
    // branch. The field itself is ignored by the notifier but the visitor is
    // invoked.
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!(error = ?DebuggableError, "captured");

    let entries = svc.get_recent_errors();
    assert_eq!(entries.len(), 1);
    // The entry's message should still contain the "captured" literal.
    assert!(entries[0].message.contains("captured"));
}

#[tokio::test]
async fn layer_records_str_field_non_message() {
    // Structured str field whose name is not `message` hits the
    // field.name() != "message" branch of `record_str`.
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!(key = "value", "event body");

    let entries = svc.get_recent_errors();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].message.contains("event body"));
}

#[tokio::test]
async fn stopped_service_after_start_drops_events() {
    // start() then stop(): events after stop should not be captured.
    let svc = started_service(15);
    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!("before stop");
    svc.stop();
    tracing::error!("after stop");

    let entries = svc.get_recent_errors();
    // Exactly one event was captured (the one before stop()).
    assert_eq!(entries.len(), 1);
    assert!(entries[0].message.contains("before stop"));
}
