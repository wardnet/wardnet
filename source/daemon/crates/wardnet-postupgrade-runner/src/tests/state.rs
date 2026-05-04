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

#[test]
fn invalid_json_falls_back_to_default_state() {
    // If state.json is corrupted (e.g. truncated mid-write), we
    // discard the contents and overwrite with a fresh record rather
    // than refusing to mark the verification failure. The lost
    // applied/failed history is the lesser evil — the journal entry
    // still exists, and the next migration runner exit will rebuild
    // applied[]/failed[] correctly.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, b"this is not json").unwrap();

    let now = chrono::Utc::now();
    record_verification_failure(&path, "boom", now).expect("recover from invalid json");

    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(v["last_verification_failure"]["error"], "boom");
}

#[test]
fn read_error_other_than_not_found_returns_err() {
    // Pass a directory as state_path. `std::fs::read` returns an
    // Err with kind that's not `NotFound`, exercising the third
    // match arm that surfaces I/O errors instead of silently
    // defaulting.
    let dir = TempDir::new().unwrap();
    let path_as_directory = dir.path().join("state-as-dir");
    std::fs::create_dir(&path_as_directory).unwrap();

    let now = chrono::Utc::now();
    let err = record_verification_failure(&path_as_directory, "boom", now)
        .expect_err("reading a directory must surface as Err");
    let chain = format!("{err:#}");
    assert!(
        chain.contains("could not read existing state"),
        "expected read-error context, got: {chain}"
    );
}
