//! Tests for the polkit power-rule migration reconciler.

use std::fs;

use crate::up::polkit::{RULE_BODY, RULE_FILENAME, write_rule};

use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

#[test]
fn writes_expected_content_and_mode() {
    let dir = TempDir::new().unwrap();
    write_rule(dir.path()).unwrap();

    let file = dir.path().join(RULE_FILENAME);
    let bytes = fs::read(&file).unwrap();
    assert_eq!(bytes, RULE_BODY.as_bytes());

    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644, "expected 0644, got {mode:o}");
}

#[test]
fn rule_body_authorises_wardnet_user_for_login1_actions() {
    // Spot-check the embedded rule body — guards against an
    // accidental edit that no longer matches the four action ids
    // the daemon needs.
    assert!(RULE_BODY.contains("subject.user !== \"wardnet\""));
    assert!(RULE_BODY.contains("org.freedesktop.login1.reboot"));
    assert!(RULE_BODY.contains("org.freedesktop.login1.reboot-multiple-sessions"));
    assert!(RULE_BODY.contains("org.freedesktop.login1.power-off"));
    assert!(RULE_BODY.contains("org.freedesktop.login1.power-off-multiple-sessions"));
    assert!(RULE_BODY.contains("polkit.Result.YES"));
}

#[test]
fn idempotent_rerun_preserves_mtime() {
    let dir = TempDir::new().unwrap();
    write_rule(dir.path()).unwrap();
    let file = dir.path().join(RULE_FILENAME);
    let mtime_before = fs::metadata(&file).unwrap().modified().unwrap();

    // Sleep briefly so a rewrite would produce a different mtime
    // even on filesystems with second-resolution timestamps.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    write_rule(dir.path()).unwrap();
    let mtime_after = fs::metadata(&file).unwrap().modified().unwrap();
    assert_eq!(
        mtime_before, mtime_after,
        "second run rewrote the file even though content matched"
    );
}

#[test]
fn mismatched_content_is_rewritten() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join(RULE_FILENAME);
    fs::write(&file, b"// stale content from a hand edit\n").unwrap();

    write_rule(dir.path()).unwrap();
    let bytes = fs::read(&file).unwrap();
    assert_eq!(bytes, RULE_BODY.as_bytes());
}

#[test]
fn idempotent_rerun_restores_mode_if_chmodded() {
    let dir = TempDir::new().unwrap();
    write_rule(dir.path()).unwrap();
    let file = dir.path().join(RULE_FILENAME);

    // Operator chmods to 0600 by mistake; re-running the
    // migration should put it back to 0644 without rewriting
    // the content.
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    write_rule(dir.path()).unwrap();

    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644);
}

#[test]
fn creates_base_directory_if_missing() {
    let parent = TempDir::new().unwrap();
    let base = parent.path().join("not-yet-created");
    write_rule(&base).unwrap();

    assert!(base.join(RULE_FILENAME).exists());
}
