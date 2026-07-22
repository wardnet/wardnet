//! Idempotent reconciler for `/etc/systemd/system/wardnetd.service`.
//!
//! The auto-updater ships only binaries (plus `wardnet-postupgrade.service`),
//! never the `wardnetd.service` unit — so a unit-file fix would otherwise
//! reach existing installs only when an operator re-runs `install.sh`. This
//! step closes that gap: it rewrites the unit from the canonical copy baked
//! into this release and reloads systemd.
//!
//! Scope note: like every migration here this runs **once** (the runner
//! records it applied, then skips it). So it propagates the unit exactly as
//! it stood *when this migration was added* — it is not a standing sync of
//! `deploy/wardnetd.service`. A later change to the unit that must reach
//! existing installs needs its own follow-up migration with a new id, which
//! is the framework's append-only model (see [`crate::migrations`]).

use std::path::Path;
use std::process::Command;

use anyhow::Context;

use crate::up::write_file_atomic;

/// The unit filename under the systemd unit dir.
pub const UNIT_FILENAME: &str = "wardnetd.service";

/// Default systemd unit directory. Tests pass tempdir paths.
pub const DEFAULT_UNIT_DIR: &str = "/etc/systemd/system";

/// Canonical unit text, baked in at compile time from the single source
/// of truth `deploy/wardnetd.service` — the very file `install.sh` lays
/// down on a fresh install. Embedding it (rather than a hand-kept copy)
/// makes drift impossible: there is exactly one unit definition in the
/// tree. The path mirrors the release-pubkey include in `wardnetd-services`
/// and resolves via `COPY deploy/ /deploy/` in the build/test images.
pub const UNIT_BODY: &str = include_str!("../../../../../../deploy/wardnetd.service");

/// World-readable, matching how `install.sh` writes the unit (`0644`).
const UNIT_MODE: u32 = 0o644;

/// Write the canonical unit to `<base>/wardnetd.service`. Returns whether
/// the file changed, so [`reconcile`] can skip `daemon-reload` on a no-op.
pub fn write_unit(base: &Path) -> anyhow::Result<bool> {
    write_file_atomic(base, UNIT_FILENAME, UNIT_BODY, UNIT_MODE)
}

/// Write the unit, then reload systemd so the definition takes effect this
/// boot. Runs before `wardnetd.service` starts (this binary is `RequiredBy`
/// it), so the reload never races a running daemon.
pub fn reconcile(base: &Path) -> anyhow::Result<()> {
    write_unit(base)?;

    // A tempdir base (tests) must not shell out to systemd.
    if base == Path::new(DEFAULT_UNIT_DIR) {
        daemon_reload()?;
    }
    Ok(())
}

/// `systemctl daemon-reload` so the rewritten unit takes effect on this boot.
///
/// Deliberately **not** gated on whether the file changed, and a genuine
/// reload failure is **propagated** rather than swallowed. This migration
/// runs at most once (the runner records it applied), so a swallowed failure
/// would leave the fix inert until the next reboot with no retry. Propagating
/// records the Optional migration as failed, which re-runs it next boot — and
/// because the reload isn't gated on `changed`, that retry actually reloads
/// even though the file already matches. A *missing* `systemctl` (a
/// non-systemd host that somehow reached this path) is treated as success:
/// there is nothing to reload.
fn daemon_reload() -> anyhow::Result<()> {
    match Command::new("systemctl").arg("daemon-reload").status() {
        Ok(status) if status.success() => {
            tracing::info!("reloaded systemd after refreshing wardnetd.service");
            Ok(())
        }
        Ok(status) => {
            anyhow::bail!("systemctl daemon-reload exited with {status}")
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::info!("systemctl not found; skipping daemon-reload (non-systemd host)");
            Ok(())
        }
        Err(e) => Err(e).context("running systemctl daemon-reload"),
    }
}
