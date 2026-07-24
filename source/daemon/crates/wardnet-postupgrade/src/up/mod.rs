//! Migration `up` functions.
//!
//! Each function in this module corresponds to one entry in
//! [`crate::migrations::migrations()`]. Functions are **idempotent
//! reconcilers** — running the same migration twice must leave the
//! system in the same state and must not flip the file's mtime if no
//! content change is required.
//!
//! Functions exposed at this level take no arguments so they fit the
//! `fn() -> anyhow::Result<()>` shape expected by [`crate::Migration`].
//! The actual logic lives in submodules that accept a base-path
//! argument so unit tests can drive them against a `tempfile::TempDir`.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Context;

pub mod polkit;
pub mod systemd_unit;
pub mod watchdog;

#[cfg(test)]
mod tests;

/// Migration `0001_polkit_power_rule` entry point.
///
/// Writes `/etc/polkit-1/rules.d/50-wardnet-power.rules` so the
/// unprivileged `wardnet` user can call
/// `org.freedesktop.login1.{Reboot,PowerOff}` (plus the
/// `*-multiple-sessions` variants) — what `systemctl reboot` and
/// `systemctl poweroff` actually invoke under the hood.
pub fn write_polkit_power_rule() -> anyhow::Result<()> {
    polkit::write_rule(Path::new(polkit::DEFAULT_RULES_DIR))
}

/// Migration `0002_watchdog_udev_rule` entry point.
///
/// Installs the udev rule that hands `/dev/watchdog` to the `wardnet`
/// group so the unprivileged daemon can arm the ungated hardware-reboot
/// backstop (issue #214). The persistent rule handles every future
/// boot; a best-effort udev retrigger + device chgrp also arms it on
/// the current boot without waiting for a reboot.
pub fn write_watchdog_udev_rule() -> anyhow::Result<()> {
    watchdog::reconcile(Path::new(watchdog::DEFAULT_RULES_DIR))
}

/// Migration `0003_wardnetd_unit_refresh` entry point.
///
/// Reconciles `/etc/systemd/system/wardnetd.service` with the canonical
/// unit shipped in this release, then reloads systemd. This is how a
/// unit-file fix (e.g. the `StartLimit*` crash-loop budget that only takes
/// effect under `[Unit]`) reaches installs that update via the auto-updater —
/// the update tarball carries only binaries, never the service units.
///
/// Runs once (like every migration), so it propagates the unit as it stood
/// when this migration was added; a *future* unit change that must reach
/// existing installs needs its own follow-up migration, not an edit here.
pub fn refresh_wardnetd_unit() -> anyhow::Result<()> {
    systemd_unit::reconcile(Path::new(systemd_unit::DEFAULT_UNIT_DIR))
}

/// Atomically write `body` to `<base>/<filename>` with mode `mode`.
///
/// Idempotent: if the target already holds identical bytes, the mode is
/// reconciled (operators sometimes hand-chmod files) and `Ok(false)` is
/// returned **without touching mtime**. Otherwise the content is written
/// to a sibling temp file, fsynced, renamed into place, and the parent
/// directory is fsynced — so a power loss after the rename leaves the new
/// content (not zeroes) on disk — and `Ok(true)` is returned.
///
/// The returned bool lets callers skip a follow-up reload (udev /
/// `systemctl daemon-reload`) when nothing actually changed.
pub(crate) fn write_file_atomic(
    base: &Path,
    filename: &str,
    body: &str,
    mode: u32,
) -> anyhow::Result<bool> {
    let target = base.join(filename);

    // Idempotent fast path: bytes already match. Don't touch mtime;
    // still reconcile the mode in case it was hand-edited.
    if let Ok(existing) = fs::read(&target)
        && existing == body.as_bytes()
    {
        ensure_mode(&target, mode)?;
        return Ok(false);
    }

    fs::create_dir_all(base).with_context(|| format!("creating dir {}", base.display()))?;

    // Sibling temp file so the rename is atomic on the same fs. A
    // leading '.' keeps the partial file out of any inotify watcher's
    // attention (udev, polkitd) until the rename onto the final name.
    let tmp = base.join(format!(".{filename}.tmp"));
    {
        let mut file = fs::File::create(&tmp)
            .with_context(|| format!("creating temp file {}", tmp.display()))?;
        file.write_all(body.as_bytes())
            .with_context(|| format!("writing temp file {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("fsync temp file {}", tmp.display()))?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(mode))
            .with_context(|| format!("chmod temp file {}", tmp.display()))?;
    }

    fs::rename(&tmp, &target)
        .with_context(|| format!("rename {} -> {}", tmp.display(), target.display()))?;
    fsync_dir(base)?;

    Ok(true)
}

/// Ensure `path` has mode `mode`; a no-op if it already does.
fn ensure_mode(path: &Path, mode: u32) -> anyhow::Result<()> {
    let meta = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if meta.permissions().mode() & 0o777 != mode {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .with_context(|| format!("chmod {}", path.display()))?;
    }
    Ok(())
}

/// Open the directory and call `fsync(2)` on it so a preceding rename is
/// durable before we return.
fn fsync_dir(dir: &Path) -> anyhow::Result<()> {
    let f = fs::File::open(dir).with_context(|| format!("open dir {}", dir.display()))?;
    f.sync_all()
        .with_context(|| format!("fsync dir {}", dir.display()))?;
    Ok(())
}
