//! Tests for [`LogServiceImpl`] — the orchestrator that combines the log
//! stream and error notifier behind a single unified trait.

use std::path::PathBuf;
use std::sync::Arc;

use tracing_subscriber::layer::SubscriberExt;
use uuid::Uuid;

use crate::logging::error_notifier::ErrorNotifierService;
use crate::logging::service::{LogService, LogServiceImpl};
use crate::logging::stream::LogStreamService;

/// Build a fresh log service with a unique temp log path and default capacities.
fn build_service() -> (LogServiceImpl, PathBuf) {
    let stream = Arc::new(LogStreamService::new(64));
    let errors = Arc::new(ErrorNotifierService::new(15));
    // Use a per-test unique directory so parallel tests don't collide.
    let dir = std::env::temp_dir().join(format!("wardnet-test-logs-{}", Uuid::new_v4()));
    let log_path = dir.join("wardnetd.log");
    (LogServiceImpl::new(stream, errors, log_path.clone()), dir)
}

#[test]
fn new_service_is_constructable() {
    let (_svc, _dir) = build_service();
}

#[test]
fn tracing_layers_returns_two_layers() {
    let (svc, _dir) = build_service();
    let layers = svc.tracing_layers();
    assert_eq!(
        layers.len(),
        2,
        "stream + error_notifier components expected"
    );
}

#[test]
fn start_all_then_stop_all_is_idempotent() {
    let (svc, _dir) = build_service();
    svc.start_all();
    svc.stop_all();
    svc.start_all();
    svc.stop_all();
}

#[test]
fn subscribe_returns_receiver() {
    let (svc, _dir) = build_service();
    // Ensure the service is active so events flow.
    svc.start_all();
    let _rx = svc.subscribe();
}

#[test]
fn get_recent_errors_is_empty_initially() {
    let (svc, _dir) = build_service();
    assert!(svc.get_recent_errors().is_empty());
}

#[tokio::test]
async fn list_log_files_missing_directory_errors() {
    let (svc, dir) = build_service();
    let res = svc.list_log_files().await;
    // Directory does not exist — should surface an Internal error.
    assert!(res.is_err(), "expected error for missing directory");
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn list_log_files_returns_files() {
    let (svc, dir) = build_service();
    // Create the directory and a fake log file matching the prefix.
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let active = dir.join("wardnetd.log");
    tokio::fs::write(&active, b"hello").await.unwrap();
    let rotated = dir.join("wardnetd.log.2026-04-12");
    tokio::fs::write(&rotated, b"older").await.unwrap();

    let files = svc.list_log_files().await.unwrap();
    assert!(files.len() >= 2);
    // Exactly one file should be marked active.
    assert_eq!(files.iter().filter(|f| f.active).count(), 1);

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn download_log_file_returns_content() {
    let (svc, dir) = build_service();
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let active = dir.join("wardnetd.log");
    // Non-JSON lines pass through verbatim.
    tokio::fs::write(&active, b"plain text line\n")
        .await
        .unwrap();

    let content = svc.download_log_file(None).await.unwrap();
    assert!(content.contains("plain text line"));

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn download_log_file_rejects_path_traversal() {
    let (svc, dir) = build_service();
    tokio::fs::create_dir_all(&dir).await.unwrap();

    // A name with a traversal segment should be rejected.
    let res = svc.download_log_file(Some("../etc/passwd")).await;
    assert!(res.is_err(), "path traversal should be rejected");

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn layers_published_via_service_capture_events() {
    let (svc, _dir) = build_service();
    svc.start_all();
    let mut rx = svc.subscribe();

    // The subscriber type would be awkward to spell with two boxed layers, so
    // compose the Vec<BoxedLayer> directly — `Vec<L>` implements `Layer<S>`.
    let layers = svc.tracing_layers();
    let subscriber = tracing_subscriber::registry().with(layers);
    let _guard = tracing::subscriber::set_default(subscriber);

    tracing::error!(target: "t", "service-level error");

    // Stream receives the event.
    let entry = rx.recv().await.unwrap();
    assert_eq!(entry.level, "ERROR");
    // Error notifier captures it too.
    let recent = svc.get_recent_errors();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].level, "ERROR");
}

#[tokio::test]
async fn download_log_file_by_name_returns_content() {
    // Exercise the `Some(name)` branch of `download_log_file`, which resolves
    // the name relative to the log directory (without traversal).
    let (svc, dir) = build_service();
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let rotated = dir.join("wardnetd.log.2026-04-12");
    tokio::fs::write(&rotated, b"rotated line\n").await.unwrap();

    let content = svc
        .download_log_file(Some("wardnetd.log.2026-04-12"))
        .await
        .unwrap();
    assert!(content.contains("rotated line"));

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn download_log_file_formats_json_lines() {
    // JSON-formatted tracing output should be reformatted to the
    // `timestamp LEVEL target message` layout.
    let (svc, dir) = build_service();
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let active = dir.join("wardnetd.log");
    let json_line = r#"{"timestamp":"2026-04-17T12:00:00Z","level":"INFO","target":"wardnetd","fields":{"message":"hello world"}}"#;
    tokio::fs::write(&active, format!("{json_line}\n"))
        .await
        .unwrap();

    let content = svc.download_log_file(None).await.unwrap();
    assert!(content.contains("2026-04-17T12:00:00Z"));
    assert!(content.contains("INFO"));
    assert!(content.contains("wardnetd"));
    assert!(content.contains("hello world"));

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn download_log_file_missing_file_errors() {
    // Name is valid but file does not exist: should return an Internal error
    // from the read_to_string failure.
    let (svc, dir) = build_service();
    tokio::fs::create_dir_all(&dir).await.unwrap();

    let res = svc.download_log_file(Some("wardnetd.log.absent")).await;
    assert!(res.is_err(), "missing file should error");

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn download_log_file_no_files_not_found() {
    // With `None` and an empty directory, `discover_log_files` returns an
    // empty Vec and `find_active_log_path` yields None → NotFound.
    let (svc, dir) = build_service();
    tokio::fs::create_dir_all(&dir).await.unwrap();

    let res = svc.download_log_file(None).await;
    assert!(res.is_err(), "no files should surface an error");

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn list_log_files_ignores_unrelated_files() {
    // Files that do not match the configured prefix should be filtered out.
    let (svc, dir) = build_service();
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join("wardnetd.log"), b"a")
        .await
        .unwrap();
    tokio::fs::write(dir.join("unrelated.txt"), b"b")
        .await
        .unwrap();

    let files = svc.list_log_files().await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "wardnetd.log");
    assert!(files[0].active);

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn list_log_files_picks_newest_as_active_when_configured_missing() {
    // If the configured path itself is absent but rotated files exist,
    // the most recent rotated file is flagged active.
    let (svc, dir) = build_service();
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let older = dir.join("wardnetd.log.2026-04-10");
    let newer = dir.join("wardnetd.log.2026-04-12");
    tokio::fs::write(&older, b"older").await.unwrap();
    tokio::fs::write(&newer, b"newer").await.unwrap();

    let files = svc.list_log_files().await.unwrap();
    assert_eq!(files.len(), 2);
    // Exactly one should be active. It must be the lexicographically-greatest
    // (the one find_active_log_path picks when configured file is absent).
    let active: Vec<_> = files.iter().filter(|f| f.active).collect();
    assert_eq!(active.len(), 1);

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn list_log_files_orders_by_modified_desc() {
    let (svc, dir) = build_service();
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join("wardnetd.log"), b"active")
        .await
        .unwrap();
    tokio::fs::write(dir.join("wardnetd.log.2026-04-12"), b"rot1")
        .await
        .unwrap();
    tokio::fs::write(dir.join("wardnetd.log.2026-04-11"), b"rot2")
        .await
        .unwrap();

    let files = svc.list_log_files().await.unwrap();
    // Sorted newest-first: each entry's modified_at should be >= the next.
    for pair in files.windows(2) {
        assert!(pair[0].modified_at >= pair[1].modified_at);
    }

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn start_all_activates_both_components() {
    use crate::logging::component::LogComponent;
    let stream = Arc::new(LogStreamService::new(8));
    let errors = Arc::new(ErrorNotifierService::new(8));
    let svc = LogServiceImpl::new(
        stream.clone(),
        errors.clone(),
        PathBuf::from("/tmp/wardnet-test.log"),
    );

    assert!(!stream.is_active());
    assert!(!errors.is_active());

    svc.start_all();
    assert!(stream.is_active());
    assert!(errors.is_active());

    svc.stop_all();
    assert!(!stream.is_active());
    assert!(!errors.is_active());
}

#[test]
fn log_file_info_serializes_fields() {
    use crate::logging::service::LogFileInfo;
    let info = LogFileInfo {
        name: "wardnetd.log".to_owned(),
        size_bytes: 42,
        modified_at: chrono::Utc::now(),
        active: true,
    };
    let json = serde_json::to_value(&info).unwrap();
    assert_eq!(json["name"], "wardnetd.log");
    assert_eq!(json["size_bytes"], 42);
    assert_eq!(json["active"], true);
    assert!(json["modified_at"].is_string());
}
