//! Tests for [`RecentErrors`] ring buffer and [`RecentErrorsLayer`].

use crate::recent_errors::{RecentErrors, RecentErrorsLayer};
use tracing_subscriber::layer::SubscriberExt;

// ---------------------------------------------------------------------------
// RecentErrors ring buffer unit tests
// ---------------------------------------------------------------------------

#[test]
fn new_buffer_is_empty() {
    let buf = RecentErrors::new();
    let entries = buf.get_recent();
    assert!(entries.is_empty());
}

#[test]
fn default_buffer_is_empty() {
    let buf = RecentErrors::default();
    let entries = buf.get_recent();
    assert!(entries.is_empty());
}

#[test]
fn clone_shares_underlying_data() {
    let buf = RecentErrors::new();
    let clone = buf.clone();
    // Both should see the same (empty) state.
    assert!(buf.get_recent().is_empty());
    assert!(clone.get_recent().is_empty());
}

// ---------------------------------------------------------------------------
// RecentErrorsLayer integration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn layer_captures_error_events() {
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!(target: "test", "something went wrong");

    let entries = buf.get_recent();
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
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::warn!(target: "warn_target", "a warning");

    let entries = buf.get_recent();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].level, "WARN");
    assert_eq!(entries[0].target, "warn_target");
}

#[tokio::test]
async fn layer_ignores_info_events() {
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::info!("just info");

    let entries = buf.get_recent();
    assert!(entries.is_empty(), "INFO should not be captured");
}

#[tokio::test]
async fn layer_ignores_debug_events() {
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::debug!("debug details");

    let entries = buf.get_recent();
    assert!(entries.is_empty(), "DEBUG should not be captured");
}

#[tokio::test]
async fn layer_ignores_trace_events() {
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::trace!("trace details");

    let entries = buf.get_recent();
    assert!(entries.is_empty(), "TRACE should not be captured");
}

#[tokio::test]
async fn layer_captures_both_warn_and_error() {
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::warn!("first warning");
    tracing::error!("first error");
    tracing::warn!("second warning");

    let entries = buf.get_recent();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].level, "WARN");
    assert_eq!(entries[1].level, "ERROR");
    assert_eq!(entries[2].level, "WARN");
}

#[tokio::test]
async fn layer_populates_timestamp() {
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!("ts check");

    let entries = buf.get_recent();
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].timestamp.is_empty());
    assert!(entries[0].timestamp.contains('T'));
    assert!(entries[0].timestamp.ends_with('Z'));
}

#[tokio::test]
async fn buffer_evicts_oldest_when_at_capacity() {
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    // MAX_ENTRIES is 15 (defined in recent_errors.rs).
    for i in 0..20 {
        tracing::error!("error {}", i);
    }

    let entries = buf.get_recent();
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
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    // Using a string literal message exercises record_str on the "message" field.
    tracing::error!("string literal message");

    let entries = buf.get_recent();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].message.contains("string literal message"));
}

#[tokio::test]
async fn recent_error_fields_are_populated() {
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!(target: "wardnetd::api", "handler failed");

    let entries = buf.get_recent();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.level, "ERROR");
    assert_eq!(entry.target, "wardnetd::api");
    assert!(entry.message.contains("handler failed"));
    assert!(!entry.timestamp.is_empty());
}

#[test]
fn recent_error_serializes_to_json() {
    use crate::recent_errors::RecentError;

    let entry = RecentError {
        timestamp: "2026-04-13T00:00:00.000Z".to_owned(),
        level: "ERROR".to_owned(),
        target: "wardnetd".to_owned(),
        message: "test error".to_owned(),
    };

    let json = serde_json::to_value(&entry).unwrap();
    assert_eq!(json["level"], "ERROR");
    assert_eq!(json["message"], "test error");
    assert_eq!(json["target"], "wardnetd");
    assert_eq!(json["timestamp"], "2026-04-13T00:00:00.000Z");
}

#[test]
fn recent_error_debug_impl() {
    use crate::recent_errors::RecentError;

    let entry = RecentError {
        timestamp: "t".to_owned(),
        level: "WARN".to_owned(),
        target: "t".to_owned(),
        message: "msg".to_owned(),
    };
    let debug = format!("{entry:?}");
    assert!(debug.contains("RecentError"));
}

#[tokio::test]
async fn entries_ordered_oldest_to_newest() {
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!("first");
    tracing::error!("second");
    tracing::error!("third");

    let entries = buf.get_recent();
    assert_eq!(entries.len(), 3);
    assert!(entries[0].message.contains("first"));
    assert!(entries[1].message.contains("second"));
    assert!(entries[2].message.contains("third"));
}

#[tokio::test]
async fn mixed_levels_only_captures_warn_and_error() {
    let buf = RecentErrors::new();
    let layer = RecentErrorsLayer::new(buf.clone());
    let subscriber = tracing_subscriber::registry().with(layer);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::trace!("trace");
    tracing::debug!("debug");
    tracing::info!("info");
    tracing::warn!("warn");
    tracing::error!("error");

    let entries = buf.get_recent();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].level, "WARN");
    assert_eq!(entries[1].level, "ERROR");
}
