//! Tests for the wardnetd.service refresh migration reconciler.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use crate::up::systemd_unit::{UNIT_BODY, UNIT_FILENAME, reconcile, reload_via, write_unit};

#[test]
fn writes_expected_content_and_mode() {
    let dir = TempDir::new().unwrap();
    let changed = write_unit(dir.path()).unwrap();
    assert!(changed, "first write should report a change");

    let file = dir.path().join(UNIT_FILENAME);
    assert_eq!(fs::read(&file).unwrap(), UNIT_BODY.as_bytes());

    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644, "expected 0644, got {mode:o}");
}

#[test]
fn idempotent_rerun_reports_no_change() {
    let dir = TempDir::new().unwrap();
    assert!(
        write_unit(dir.path()).unwrap(),
        "first write changes the file"
    );
    assert!(
        !write_unit(dir.path()).unwrap(),
        "second identical write should report no change"
    );
}

#[test]
fn mismatched_content_is_rewritten() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join(UNIT_FILENAME);
    fs::write(&file, b"[Unit]\nDescription=stale\n").unwrap();

    let changed = write_unit(dir.path()).unwrap();
    assert!(changed, "content mismatch should be rewritten");
    assert_eq!(fs::read(&file).unwrap(), UNIT_BODY.as_bytes());
}

#[test]
fn reconcile_against_tempdir_writes_unit_without_shelling_out() {
    // base != DEFAULT_UNIT_DIR, so reconcile must skip `systemctl
    // daemon-reload` and simply lay down the unit.
    let dir = TempDir::new().unwrap();
    reconcile(dir.path()).unwrap();
    assert_eq!(
        fs::read(dir.path().join(UNIT_FILENAME)).unwrap(),
        UNIT_BODY.as_bytes()
    );
}

#[test]
fn reload_via_success_is_ok() {
    // `true daemon-reload` exits 0 → the success arm.
    reload_via("true").unwrap();
}

#[test]
fn reload_via_nonzero_exit_is_err() {
    // `false daemon-reload` exits 1 → propagated so the migration retries.
    assert!(reload_via("false").is_err());
}

#[test]
fn reload_via_missing_binary_is_ok() {
    // A non-systemd host with no `systemctl` → tolerated (nothing to reload).
    reload_via("wardnet-no-such-systemctl-zzz").unwrap();
}

/// Regression guard for the incident that motivated this migration.
///
/// systemd only honours `StartLimit*` under `[Unit]`; placed under
/// `[Service]` it logs an "Unknown key ... in section `[Service]`"
/// warning and silently disables the crash-loop budget **and** the
/// `OnFailure=` rollback. `UNIT_BODY` *is* `deploy/wardnetd.service`
/// (compile-time include), so this asserts the shipped unit keeps the fix.
#[test]
fn start_limit_keys_live_in_unit_section_not_service() {
    let unit = section_body(UNIT_BODY, "[Unit]");
    let service = section_body(UNIT_BODY, "[Service]");

    for key in [
        "StartLimitIntervalSec",
        "StartLimitBurst",
        "StartLimitAction",
    ] {
        assert!(
            has_directive(&unit, key),
            "{key} must be a directive in the [Unit] section"
        );
        assert!(
            !has_directive(&service, key),
            "{key} must NOT be a directive in [Service] — systemd ignores it there"
        );
    }
}

/// `ProtectSystem=strict` mounts every path outside `ReadWritePaths`
/// read-only, `/etc` included. Restoring a backup rewrites the live config —
/// `BackupService::apply_import` renames `/etc/wardnet/wardnet.toml` aside to
/// a `.bak-<timestamp>` sibling and moves the bundle's copy into its place,
/// both of which need write access to the *directory*. Without the grant that
/// fails with EROFS on real hardware.
///
/// Neither end-to-end suite can catch this: the container image drops in
/// `ProtectSystem=no` (see `Dockerfile.base`), so a restore succeeds there no
/// matter what the unit says. This test is the only guard.
#[test]
fn config_directory_is_writable_for_backup_restore() {
    let service = section_body(UNIT_BODY, "[Service]");
    assert!(
        has_directive(&service, "ProtectSystem"),
        "test assumes the unit hardens the filesystem; if that was dropped, \
         this assertion is obsolete rather than merely failing"
    );

    let grants: Vec<&str> = service
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("ReadWritePaths="))
        .flat_map(str::split_whitespace)
        .collect();
    assert!(
        grants.contains(&"/etc/wardnet"),
        "backup restore writes /etc/wardnet/wardnet.toml; ReadWritePaths is {grants:?}"
    );
}

/// Collect the lines of the named section (until the next `[Header]`).
fn section_body(unit: &str, header: &str) -> String {
    let mut out = String::new();
    let mut in_section = false;
    for line in unit.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_section = trimmed == header;
            continue;
        }
        if in_section {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// True if `key=` appears as a real directive (ignoring comment lines).
fn has_directive(body: &str, key: &str) -> bool {
    body.lines().any(|line| {
        let t = line.trim();
        !t.starts_with('#') && t.starts_with(key) && t[key.len()..].trim_start().starts_with('=')
    })
}
