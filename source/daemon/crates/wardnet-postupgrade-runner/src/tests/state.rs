//! Tests for state.rs's `record_verification_failure`.

use chrono::TimeZone;
use tempfile::TempDir;

use crate::state::{record_runner_failure, record_verification_failure};

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
    // Seed a schema-valid history exactly as the migration runner
    // writes it, so the preservation contract is proven through the
    // typed loader, not just against loosely-shaped fixture JSON.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(
        &path,
        br#"{
            "applied": [{"id": "0001_keep_me", "applied_at": "2026-05-01T00:00:00Z"}],
            "failed":  [{"id": "0002_keep_me_too", "error": "exit 1", "at": "2026-05-02T00:00:00Z"}]
        }"#,
    )
    .unwrap();

    let now = chrono::Utc.with_ymd_and_hms(2026, 5, 4, 12, 0, 0).unwrap();
    record_verification_failure(&path, "tamper", now).unwrap();

    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(v["applied"][0]["id"], "0001_keep_me");
    assert_eq!(v["failed"][0]["id"], "0002_keep_me_too");
    assert_eq!(v["last_verification_failure"]["error"], "tamper");

    let state = wardnet_postupgrade::State::load(&path)
        .expect("migration runner must parse state with preserved history");
    assert!(state.is_applied("0001_keep_me"));
    assert_eq!(state.failed[0].id, "0002_keep_me_too");
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
    // The fresh state must use empty arrays, not `null` — the
    // migration runner's typed schema rejects `null` for its `Vec`
    // fields, and a `null` here would keep it exiting nonzero (and
    // wardnetd blocked) long after the original failure was resolved.
    assert!(v["applied"].is_array(), "applied must be [], got {v}");
    assert!(v["failed"].is_array(), "failed must be [], got {v}");
}

#[test]
fn corrupt_recovery_output_loads_via_migration_runner_schema() {
    // Cross-crate contract: the two State schemas are kept in sync by
    // hand, and the corrupt-recovery path once wrote `applied: null`,
    // which `wardnet-postupgrade::State::load` rejects — a transient
    // corrupt file plus one verification failure became a permanent
    // boot block. Round-trip through the real typed loader to pin the
    // contract down.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, b"{ truncated garbage").unwrap();

    let now = chrono::Utc::now();
    record_verification_failure(&path, "boom", now).expect("recover from corrupt json");

    let state = wardnet_postupgrade::State::load(&path)
        .expect("migration runner must parse state written by the recovery path");
    assert!(state.applied.is_empty());
    assert!(state.failed.is_empty());
    let failure = state
        .last_verification_failure
        .expect("failure record preserved");
    assert_eq!(failure.error, "boom");
}

#[test]
fn explicit_null_arrays_are_healed_to_empty_arrays() {
    // Older runner versions recovering from a corrupt file wrote
    // "applied": null / "failed": null — the exact shape the migration
    // runner's typed schema rejects. Such a file must not survive
    // another recording round-trip: the Vec-typed fields refuse the
    // null on parse, the start-fresh branch takes over, and the
    // rewritten file loads cleanly again.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, br#"{"applied": null, "failed": null}"#).unwrap();

    let now = chrono::Utc::now();
    record_verification_failure(&path, "boom", now).expect("recover from legacy null arrays");

    let state = wardnet_postupgrade::State::load(&path)
        .expect("migration runner must parse the healed state");
    assert!(state.applied.is_empty());
    assert!(state.failed.is_empty());
}

#[test]
fn absent_array_fields_are_rewritten_as_empty_arrays() {
    // Same contract for a structurally-valid file that lacks the
    // applied/failed keys: serde's field defaults must fill them with
    // `[]`, never `null`, so the rewritten file still loads through
    // the migration runner's typed schema.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, br#"{"last_verification_failure": null}"#).unwrap();

    let now = chrono::Utc::now();
    record_verification_failure(&path, "boom", now).expect("record over sparse state");

    let state = wardnet_postupgrade::State::load(&path)
        .expect("migration runner must parse state with defaulted arrays");
    assert!(state.applied.is_empty());
    assert!(state.failed.is_empty());
}

#[test]
fn runner_failure_round_trips_through_migration_runner_schema() {
    // Non-verification failures (artifact read, swap, exec) are
    // recorded next to verification failures and must survive both
    // directions of the manually-synced schema: written here, loaded
    // by the migration runner's typed State, and not clobbering an
    // existing verification record.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    let now = chrono::Utc.with_ymd_and_hms(2026, 5, 4, 12, 0, 0).unwrap();
    record_verification_failure(&path, "bad signature", now).unwrap();
    record_runner_failure(&path, "swap", "tarball unreadable", now).unwrap();

    let state = wardnet_postupgrade::State::load(&path)
        .expect("migration runner must parse state with a runner failure");
    let failure = state.last_runner_failure.expect("runner failure recorded");
    assert_eq!(failure.stage, "swap");
    assert_eq!(failure.error, "tarball unreadable");
    let verification = state
        .last_verification_failure
        .expect("verification record preserved alongside");
    assert_eq!(verification.error, "bad signature");
}

#[test]
fn write_root_only_matches_migration_runner_copy() {
    // The 0600 writer is mirror-copied into wardnet-postupgrade (the
    // runner cannot share code with the crate it execs), same rule as
    // the schema. The schema mirror is pinned by the round-trip tests
    // above; this pins the writer: a fix applied to one copy fails CI
    // until the other copy matches byte-for-byte.
    fn helper_body(source: &str) -> &str {
        let start = source
            .find("fn write_root_only")
            .expect("write_root_only present");
        let len = source[start..]
            .find("\n}")
            .expect("write_root_only closing brace");
        &source[start..start + len + 2]
    }

    let ours = include_str!("../state.rs");
    let theirs_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../wardnet-postupgrade/src/state.rs"
    );
    let theirs = std::fs::read_to_string(theirs_path).expect("read sibling state.rs");
    assert_eq!(
        helper_body(ours),
        helper_body(&theirs),
        "write_root_only diverged between the two crates; apply the change to both copies"
    );
}

#[cfg(unix)]
#[test]
fn state_file_is_written_root_only() {
    use std::os::unix::fs::PermissionsExt;

    // state.json is documented root-only (0600). The rename carries
    // the tmp file's mode, so the rewrite must end at 0600 even when
    // the previous file — or a stale tmp from an interrupted run —
    // was left world-readable.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, b"{}").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let tmp = dir.path().join("state.json.tmp");
    std::fs::write(&tmp, b"stale").unwrap();
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();

    let now = chrono::Utc::now();
    record_verification_failure(&path, "boom", now).unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "state.json must be root-only, got {mode:o}");
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
