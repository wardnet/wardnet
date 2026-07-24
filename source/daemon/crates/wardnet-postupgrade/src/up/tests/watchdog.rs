//! Tests for the watchdog udev-rule migration reconciler.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use crate::up::watchdog::{
    RULE_BODY, RULE_FILENAME, arm_devices_in, is_watchdog_dev, reconcile, run_best_effort,
    write_rule,
};

#[test]
fn writes_expected_content_and_mode() {
    let dir = TempDir::new().unwrap();
    let changed = write_rule(dir.path()).unwrap();
    assert!(changed, "first write should report a change");

    let file = dir.path().join(RULE_FILENAME);
    assert_eq!(fs::read(&file).unwrap(), RULE_BODY.as_bytes());

    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644, "expected 0644, got {mode:o}");
}

#[test]
fn rule_body_grants_wardnet_group_rw_on_watchdog() {
    // Spot-check the embedded body — guards against an edit that no
    // longer hands the device to the wardnet group at 0660.
    assert!(RULE_BODY.contains("KERNEL==\"watchdog\""));
    assert!(RULE_BODY.contains("KERNEL==\"watchdog[0-9]*\""));
    assert!(RULE_BODY.contains("GROUP=\"wardnet\""));
    assert!(RULE_BODY.contains("MODE=\"0660\""));
}

#[test]
fn idempotent_rerun_reports_no_change_and_preserves_mtime() {
    let dir = TempDir::new().unwrap();
    write_rule(dir.path()).unwrap();
    let file = dir.path().join(RULE_FILENAME);
    let mtime_before = fs::metadata(&file).unwrap().modified().unwrap();

    // Sleep so a rewrite would show a different mtime even on
    // second-resolution filesystems.
    std::thread::sleep(std::time::Duration::from_millis(1100));

    let changed = write_rule(dir.path()).unwrap();
    assert!(!changed, "second identical write should report no change");

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
    fs::write(&file, b"# stale content from a hand edit\n").unwrap();

    let changed = write_rule(dir.path()).unwrap();
    assert!(changed, "content mismatch should be rewritten");
    assert_eq!(fs::read(&file).unwrap(), RULE_BODY.as_bytes());
}

#[test]
fn reconcile_against_tempdir_writes_rule_without_shelling_out() {
    // base != DEFAULT_RULES_DIR, so reconcile must skip the udevadm /
    // chgrp side effects and simply lay down the rule.
    let dir = TempDir::new().unwrap();
    reconcile(dir.path()).unwrap();
    assert_eq!(
        fs::read(dir.path().join(RULE_FILENAME)).unwrap(),
        RULE_BODY.as_bytes()
    );
}

#[test]
fn is_watchdog_dev_matches_watchdog_and_numbered_variants_only() {
    for yes in ["watchdog", "watchdog0", "watchdog1", "watchdog12"] {
        assert!(is_watchdog_dev(yes), "{yes} should match");
    }
    for no in [
        "",
        "watchdoge",
        "watchdog1a",
        "wdog",
        "awatchdog",
        "watchdo",
        "null",
    ] {
        assert!(!is_watchdog_dev(no), "{no} should not match");
    }
}

#[test]
fn arm_devices_in_visits_watchdog_nodes_and_ignores_missing_dir() {
    // Missing dir: early return, no panic.
    arm_devices_in(std::path::Path::new("/definitely/not/a/real/dir/xyz"));

    // A fake /dev with a mix of watchdog and non-watchdog entries. The
    // chgrp/chmod shell-outs run against these temp files (best-effort:
    // chmod succeeds, chgrp to a possibly-absent group is swallowed) — the
    // point is to exercise the enumerate → match → shell-out path.
    let dev = TempDir::new().unwrap();
    for name in ["watchdog", "watchdog0", "watchdog3", "watchdoge", "random"] {
        fs::write(dev.path().join(name), b"").unwrap();
    }
    arm_devices_in(dev.path());
    // The non-watchdog files must still exist (we never remove anything).
    assert!(dev.path().join("random").exists());
}

#[test]
fn run_best_effort_covers_success_nonzero_and_spawn_error() {
    // Success (exit 0), non-zero exit, and a binary that cannot be spawned —
    // none of these panic or propagate; they only log.
    run_best_effort("true", &[]);
    run_best_effort("false", &[]);
    run_best_effort("wardnet-no-such-binary-zzz", &["arg"]);
}
