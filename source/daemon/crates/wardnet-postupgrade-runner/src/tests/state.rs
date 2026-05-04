//! Tests for state.rs's `record_verification_failure`.

use chrono::TimeZone;
use tempfile::TempDir;

use crate::state::record_verification_failure;

#[test]
fn writes_fresh_state_when_file_absent() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    let now = chrono::Utc.with_ymd_and_hms(2026, 5, 4, 12, 0, 0).unwrap();
    record_verification_failure(&path, "boom", now).unwrap();

    let bytes = std::fs::read(&path).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["last_verification_failure"]["error"], "boom");
    assert!(v["applied"].is_array());
    assert!(v["failed"].is_array());
}

#[test]
fn preserves_applied_and_failed_arrays() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(
        &path,
        br#"{
            "applied": [{"id": "0001_keep_me"}],
            "failed":  [{"id": "0002_keep_me_too"}]
        }"#,
    )
    .unwrap();

    let now = chrono::Utc.with_ymd_and_hms(2026, 5, 4, 12, 0, 0).unwrap();
    record_verification_failure(&path, "tamper", now).unwrap();

    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(v["applied"][0]["id"], "0001_keep_me");
    assert_eq!(v["failed"][0]["id"], "0002_keep_me_too");
    assert_eq!(v["last_verification_failure"]["error"], "tamper");
}

#[test]
fn creates_parent_directory() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nested/state.json");
    let now = chrono::Utc::now();
    record_verification_failure(&path, "boom", now).unwrap();
    assert!(path.exists());
}
