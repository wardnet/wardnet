//! Tests for `state.rs`.

use chrono::TimeZone;
use tempfile::TempDir;

use crate::state::{AppliedEntry, FailedEntry, State};

#[test]
fn load_returns_default_when_file_missing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.json");
    let state = State::load(&path).expect("load missing file");
    assert!(state.applied.is_empty());
    assert!(state.failed.is_empty());
    assert!(state.last_verification_failure.is_none());
}

#[test]
fn save_then_load_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    let now = chrono::Utc.with_ymd_and_hms(2026, 5, 4, 12, 0, 0).unwrap();
    let state = State {
        applied: vec![AppliedEntry {
            id: "0001".into(),
            applied_at: now,
        }],
        failed: vec![FailedEntry {
            id: "0002".into(),
            error: "oops".into(),
            at: now,
        }],
        last_verification_failure: None,
    };
    state.save(&path).unwrap();

    let loaded = State::load(&path).unwrap();
    assert_eq!(loaded.applied.len(), 1);
    assert_eq!(loaded.applied[0].id, "0001");
    assert_eq!(loaded.failed.len(), 1);
    assert_eq!(loaded.failed[0].id, "0002");
}

#[test]
fn save_creates_parent_directories() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("a/b/state.json");
    State::default().save(&path).unwrap();
    assert!(path.exists());
}

#[test]
fn is_applied_matches_by_id() {
    let now = chrono::Utc.with_ymd_and_hms(2026, 5, 4, 12, 0, 0).unwrap();
    let state = State {
        applied: vec![AppliedEntry {
            id: "0001".into(),
            applied_at: now,
        }],
        ..Default::default()
    };
    assert!(state.is_applied("0001"));
    assert!(!state.is_applied("0002"));
}

#[cfg(unix)]
#[test]
fn save_keeps_state_file_root_only() {
    use std::os::unix::fs::PermissionsExt;

    // The module doc pins state.json at mode 0600. The write-then-
    // rename must uphold that even when overwriting a file (or a
    // stale tmp) that was left world-readable.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, b"{}").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let tmp = dir.path().join("state.json.tmp");
    std::fs::write(&tmp, b"stale").unwrap();
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();

    State::default().save(&path).expect("save");

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "state.json must be root-only, got {mode:o}");
}

#[cfg(unix)]
#[test]
fn load_reconciles_world_readable_state_to_root_only() {
    use std::os::unix::fs::PermissionsExt;

    // Older releases wrote state.json 0644 and a healthy host never
    // rewrites it, so load — which runs every boot — tightens the
    // mode back to the documented 0600.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    State::default().save(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    State::load(&path).expect("load");

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "load must tighten legacy world-readable state, got {mode:o}"
    );
}

#[test]
fn load_returns_err_on_invalid_json() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, b"definitely not json").unwrap();
    let err = State::load(&path).expect_err("invalid json must surface as Err");
    let chain = format!("{err:#}");
    assert!(
        chain.contains("parsing state file"),
        "expected parse-error context, got: {chain}"
    );
}

#[test]
fn load_returns_err_when_path_is_a_directory() {
    // `std::fs::read` on a directory returns Err with kind that is
    // not `NotFound`, exercising the third match arm in `load`.
    let dir = TempDir::new().unwrap();
    let path_as_dir = dir.path().join("state-as-dir");
    std::fs::create_dir(&path_as_dir).unwrap();
    let err = State::load(&path_as_dir).expect_err("directory must surface as Err");
    let chain = format!("{err:#}");
    assert!(
        chain.contains("reading state file"),
        "expected read-error context, got: {chain}"
    );
}
