//! Tests for [`LogStreamService`], its tracing layer, and [`LogEntry`].
//!
//! Since the underlying layer type is private, all end-to-end send/receive tests
//! exercise the full pipeline through `LogStreamService::tracing_layer()` + tracing.

use std::collections::BTreeMap;
use std::sync::Arc;

use tracing_subscriber::layer::SubscriberExt;

use crate::logging::component::LogComponent;
use crate::logging::stream::{LogEntry, LogStream, LogStreamService};

/// Build a configured-and-started stream service with capacity for tests.
fn started_service(capacity: usize) -> Arc<LogStreamService> {
    let svc = Arc::new(LogStreamService::new(capacity));
    svc.start();
    svc
}

// ---------------------------------------------------------------------------
// LogStreamService unit tests (public API only)
// ---------------------------------------------------------------------------

#[test]
fn new_creates_service_without_panic() {
    let _svc = LogStreamService::new(16);
}

#[test]
fn subscribe_returns_receiver() {
    let svc = LogStreamService::new(16);
    let _rx = svc.subscribe();
}

#[test]
fn start_marks_service_active() {
    let svc = LogStreamService::new(16);
    assert!(!svc.is_active());
    svc.start();
    assert!(svc.is_active());
}

#[test]
fn stop_marks_service_inactive() {
    let svc = LogStreamService::new(16);
    svc.start();
    svc.stop();
    assert!(!svc.is_active());
}

// ---------------------------------------------------------------------------
// LogEntry serialization
// ---------------------------------------------------------------------------

#[test]
fn log_entry_serializes_without_empty_maps() {
    let entry = LogEntry {
        timestamp: "2026-04-13T00:00:00.000Z".to_owned(),
        level: "ERROR".to_owned(),
        target: "wardnetd::api".to_owned(),
        message: "something broke".to_owned(),
        fields: BTreeMap::new(),
        span: BTreeMap::new(),
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["level"], "ERROR");
    assert_eq!(json["message"], "something broke");
    assert_eq!(json["timestamp"], "2026-04-13T00:00:00.000Z");
    assert_eq!(json["target"], "wardnetd::api");
    // Empty fields/span should be omitted (skip_serializing_if).
    assert!(json.get("fields").is_none());
    assert!(json.get("span").is_none());
}

#[test]
fn log_entry_serializes_fields_when_present() {
    let mut fields = BTreeMap::new();
    fields.insert("key".to_owned(), "val".to_owned());

    let entry = LogEntry {
        timestamp: "t".to_owned(),
        level: "INFO".to_owned(),
        target: "t".to_owned(),
        message: "m".to_owned(),
        fields,
        span: BTreeMap::new(),
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["fields"]["key"], "val");
    // span still empty, should be absent.
    assert!(json.get("span").is_none());
}

#[test]
fn log_entry_serializes_span_when_present() {
    let mut span = BTreeMap::new();
    span.insert("method".to_owned(), "GET".to_owned());

    let entry = LogEntry {
        timestamp: "t".to_owned(),
        level: "INFO".to_owned(),
        target: "t".to_owned(),
        message: "m".to_owned(),
        fields: BTreeMap::new(),
        span,
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["span"]["method"], "GET");
    assert!(json.get("fields").is_none());
}

#[test]
fn log_entry_debug_impl_does_not_panic() {
    let entry = LogEntry {
        timestamp: "t".to_owned(),
        level: "INFO".to_owned(),
        target: "t".to_owned(),
        message: "m".to_owned(),
        fields: BTreeMap::new(),
        span: BTreeMap::new(),
    };
    let debug = format!("{entry:?}");
    assert!(debug.contains("LogEntry"));
}

// ---------------------------------------------------------------------------
// Tracing layer integration tests (exercises send + receive)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn layer_captures_info_event() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info!(target: "test_target", "hello from tracing");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.level, "INFO");
    assert_eq!(received.target, "test_target");
    assert!(
        received.message.contains("hello from tracing"),
        "message was: {}",
        received.message
    );
}

#[tokio::test]
async fn layer_captures_error_event() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!(target: "err_target", "an error occurred");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.level, "ERROR");
    assert_eq!(received.target, "err_target");
}

#[tokio::test]
async fn layer_captures_warn_event() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::warn!("a warning");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.level, "WARN");
}

#[tokio::test]
async fn layer_captures_debug_event() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::debug!("debug details");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.level, "DEBUG");
}

#[tokio::test]
async fn layer_captures_structured_fields() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::warn!(user_id = 42, active = true, "user event");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.level, "WARN");
    assert_eq!(received.fields["user_id"], "42");
    assert_eq!(received.fields["active"], "true");
}

#[tokio::test]
async fn layer_captures_string_field_via_record_str() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info!(name = "test-name", "with str field");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.fields["name"], "test-name");
}

#[tokio::test]
async fn layer_captures_u64_field() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info!(count = 7_u64, "with u64 field");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.fields["count"], "7");
}

#[tokio::test]
async fn layer_captures_i64_field() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info!(offset = -10_i64, "negative field");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.fields["offset"], "-10");
}

#[tokio::test]
async fn layer_captures_bool_field() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info!(enabled = false, "bool field");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.fields["enabled"], "false");
}

#[tokio::test]
async fn layer_populates_timestamp() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info!("ts check");

    let received = rx.recv().await.unwrap();
    assert!(!received.timestamp.is_empty());
    // Should be RFC3339 format.
    assert!(received.timestamp.contains('T'));
    assert!(received.timestamp.ends_with('Z'));
}

#[tokio::test]
async fn layer_captures_span_fields() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    let span = tracing::info_span!("http_request", method = "GET", path = "/api/status");
    let _enter = span.enter();

    tracing::info!("handling request");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.span["method"], "GET");
    assert_eq!(received.span["path"], "/api/status");
}

#[tokio::test]
async fn layer_merges_nested_span_fields() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    let outer = tracing::info_span!("outer", service = "api");
    let _outer_guard = outer.enter();

    let inner = tracing::info_span!("inner", handler = "status");
    let _inner_guard = inner.enter();

    tracing::info!("nested span event");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.span["service"], "api");
    assert_eq!(received.span["handler"], "status");
}

#[tokio::test]
async fn layer_multiple_events_all_received() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!("first");
    tracing::warn!("second");
    tracing::info!("third");

    let r1 = rx.recv().await.unwrap();
    let r2 = rx.recv().await.unwrap();
    let r3 = rx.recv().await.unwrap();
    assert_eq!(r1.level, "ERROR");
    assert_eq!(r2.level, "WARN");
    assert_eq!(r3.level, "INFO");
}

#[tokio::test]
async fn layer_multiple_subscribers_all_receive() {
    let svc = started_service(64);
    let mut rx1 = svc.subscribe();
    let mut rx2 = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info!("broadcast to all");

    let r1 = rx1.recv().await.unwrap();
    let r2 = rx2.recv().await.unwrap();
    assert_eq!(r1.message, r2.message);
}

#[tokio::test]
async fn layer_span_with_i64_field() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    let span = tracing::info_span!("req", status = 200_i64);
    let _enter = span.enter();

    tracing::info!("done");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.span["status"], "200");
}

#[tokio::test]
async fn layer_span_with_u64_field() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    let span = tracing::info_span!("req", bytes = 1024_u64);
    let _enter = span.enter();

    tracing::info!("done");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.span["bytes"], "1024");
}

#[tokio::test]
async fn layer_span_with_bool_field() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    let span = tracing::info_span!("req", cached = true);
    let _enter = span.enter();

    tracing::info!("done");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.span["cached"], "true");
}

#[tokio::test]
async fn layer_on_record_updates_span_fields() {
    let svc = started_service(64);
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    let span = tracing::info_span!("req", status = tracing::field::Empty);
    let _enter = span.enter();

    // Record a value after span creation.
    span.record("status", 404_i64);

    tracing::info!("response sent");

    let received = rx.recv().await.unwrap();
    assert_eq!(received.span["status"], "404");
}

#[tokio::test]
async fn layer_inactive_does_not_publish() {
    // Service is created but never started — events should not reach subscribers.
    let svc = Arc::new(LogStreamService::new(16));
    let mut rx = svc.subscribe();

    let subscriber = tracing_subscriber::registry().with(svc.tracing_layer());
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info!("inactive event");

    // Use try_recv to confirm no value was enqueued.
    assert!(rx.try_recv().is_err());
}
