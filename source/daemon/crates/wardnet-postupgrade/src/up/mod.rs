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

use std::path::Path;

pub mod polkit;

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
