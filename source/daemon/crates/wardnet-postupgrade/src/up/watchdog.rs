//! Idempotent installer for the udev rule that grants the `wardnet`
//! group access to `/dev/watchdog` (issue #214).
//!
//! The kernel creates the device `root:root 0600`; the daemon runs as
//! `User=wardnet` with no `CAP_DAC_OVERRIDE`, so without this rule its
//! open of `/dev/watchdog` fails `EACCES` and the ungated hardware-reboot
//! backstop silently stays off. The persistent rule fixes every future
//! boot (udev applies it whenever the device appears); a best-effort
//! retrigger + `chgrp`/`chmod` of the live device also arms it on the
//! current boot without a reboot.

use std::path::Path;
use std::process::Command;

use crate::up::write_file_atomic;

/// Filename under `rules.d/`. The `99-` prefix runs it after distro
/// defaults so our ownership/mode wins.
pub const RULE_FILENAME: &str = "99-wardnet-watchdog.rules";

/// Default base directory on a real host. Tests pass tempdir paths.
pub const DEFAULT_RULES_DIR: &str = "/etc/udev/rules.d";

/// Embedded rule body — the single source of truth for the rule.
pub const RULE_BODY: &str = include_str!("watchdog_udev.rules.in");

/// World-readable, like every other rule under `rules.d/`.
const RULE_MODE: u32 = 0o644;

/// Write the udev rule to `<base>/99-wardnet-watchdog.rules`. Returns
/// whether the file changed, so [`reconcile`] can skip the udev reload
/// on a no-op.
pub fn write_rule(base: &Path) -> anyhow::Result<bool> {
    write_file_atomic(base, RULE_FILENAME, RULE_BODY, RULE_MODE)
}

/// Write the rule, then — only against the real `rules.d/` — best-effort
/// arm it on the current boot. The live-device fixups just save one
/// reboot, so their failure is logged and swallowed rather than failing
/// the (Optional) migration; the persistent rule is what matters.
pub fn reconcile(base: &Path) -> anyhow::Result<()> {
    let changed = write_rule(base)?;

    // A tempdir base (tests) must never shell out to the host.
    if base == Path::new(DEFAULT_RULES_DIR) {
        if changed {
            run_best_effort("udevadm", &["control", "--reload"]);
            run_best_effort("udevadm", &["trigger", "--name-match=watchdog"]);
        }
        arm_live_device();
    }
    Ok(())
}

/// `chgrp`/`chmod` every already-present watchdog device so the daemon can
/// open it on this boot without waiting for udev to re-fire on reboot.
///
/// Enumerates `/dev` rather than hardcoding `watchdog`/`watchdog0`: the udev
/// rule matches `watchdog[0-9]*`, and a host may expose the device the daemon
/// is configured to open as a higher-numbered node (e.g. `/dev/watchdog1` when
/// several watchdog drivers are present). Arming only a fixed pair would leave
/// that device inaccessible until the next reboot.
fn arm_live_device() {
    let Ok(entries) = std::fs::read_dir("/dev") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Match the bare `watchdog` alias and `watchdog<N>`; skip anything
        // else (e.g. `watchdog` is a prefix of nothing unrelated in /dev, but
        // be strict anyway).
        let is_watchdog = name == "watchdog"
            || name
                .strip_prefix("watchdog")
                .is_some_and(|rest| !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()));
        if is_watchdog {
            let path = entry.path();
            let path = path.to_string_lossy();
            run_best_effort("chgrp", &["wardnet", &path]);
            run_best_effort("chmod", &["0660", &path]);
        }
    }
}

/// Run a system command, logging (never propagating) any failure — the
/// caller is a best-effort convenience path.
fn run_best_effort(cmd: &str, args: &[&str]) {
    match Command::new(cmd).args(args).status() {
        Ok(status) if status.success() => {}
        Ok(status) => tracing::warn!(
            command = cmd,
            ?args,
            code = status.code(),
            "watchdog udev reconcile: {cmd} exited non-zero (non-fatal)",
        ),
        Err(e) => tracing::warn!(
            command = cmd,
            ?args,
            error = %e,
            "watchdog udev reconcile: could not run {cmd} (non-fatal): {e}",
        ),
    }
}
