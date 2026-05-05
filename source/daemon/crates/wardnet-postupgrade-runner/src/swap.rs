//! Verify-and-swap of `wardnetd` from a wardnet-staged release tarball.
//!
//! The trust anchor: this module re-verifies the outer release tarball
//! signature against the embedded public key before touching
//! `/usr/local/bin/wardnetd`. The wardnet user can stage any bytes at
//! the pending paths, but cannot bypass the signature check, so the
//! escalation surface is bounded by the release signing key.
//!
//! Two reasons the runner — not the daemon — owns this step:
//!   1. `/usr/local/bin/` is `root:root 0755`, so the unprivileged
//!      wardnet user cannot rename anything into it (rename(2) needs
//!      write+x on the destination directory). The runner is `User=root`.
//!   2. Re-verification under root closes a TOCTOU window even within
//!      the same trust domain — the wardnet user could replace the
//!      pending tarball after the daemon's pre-stage verify; verifying
//!      again here defeats that.
//!
//! On a successful swap the previously-live binary is preserved at
//! `<live>.old` so `wardnetd-rollback.service` can restore it if the
//! new daemon crash-loops.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::verify;

/// Default location of the daemon-staged release tarball awaiting the
/// privileged swap. Wardnet-writable; the runner re-verifies before
/// trusting any byte of it.
pub const DEFAULT_PENDING_TARBALL_PATH: &str = "/var/lib/wardnet/updates/wardnetd-pending.tar.gz";
/// Default location of the detached minisign signature accompanying
/// [`DEFAULT_PENDING_TARBALL_PATH`].
pub const DEFAULT_PENDING_SIGNATURE_PATH: &str =
    "/var/lib/wardnet/updates/wardnetd-pending.tar.gz.minisig";
/// Default location of the rollback-request marker. Empty file written
/// by the daemon when an operator requests a rollback; the runner
/// performs the actual rename of `<live>.old → <live>` and removes
/// the marker.
pub const DEFAULT_ROLLBACK_REQUEST_PATH: &str =
    "/var/lib/wardnet/updates/wardnetd-rollback.request";

/// File name the swap pulls from inside the release tarball. Anything
/// else in the archive is ignored — the postupgrade migration runner
/// owns the rest of the payload.
const ARCHIVE_NAME_DAEMON: &str = "wardnetd";

/// Where to find a swap request and what to do on success. Mirrors the
/// daemon-side [`FsBinaryApplier`] paths so production wires both ends
/// to the same locations.
pub struct SwapPaths {
    pub pending_tarball: PathBuf,
    pub pending_signature: PathBuf,
    pub rollback_request: PathBuf,
    pub live_binary: PathBuf,
}

impl SwapPaths {
    /// Production paths. Tests construct `SwapPaths` directly inside a
    /// tempdir.
    #[must_use]
    pub fn with_defaults(live_binary: PathBuf) -> Self {
        Self {
            pending_tarball: PathBuf::from(DEFAULT_PENDING_TARBALL_PATH),
            pending_signature: PathBuf::from(DEFAULT_PENDING_SIGNATURE_PATH),
            rollback_request: PathBuf::from(DEFAULT_ROLLBACK_REQUEST_PATH),
            live_binary,
        }
    }

    fn old_binary(&self) -> PathBuf {
        let mut p = self.live_binary.clone();
        let name = p.file_name().map_or_else(
            || "wardnetd.old".to_owned(),
            |n| format!("{}.old", n.to_string_lossy()),
        );
        p.set_file_name(name);
        p
    }
}

/// What the swap step did. The runner exits nonzero on `Failed` so
/// systemd refuses to start `wardnetd.service` until an operator
/// clears the failure — preserving the safety property that the
/// daemon never runs against a binary the trust anchor couldn't
/// verify.
#[derive(Debug)]
pub enum SwapOutcome {
    /// No pending tarball and no rollback request; the runner moves on
    /// to the migration step. The common boot path on a host that has
    /// not run an auto-update since reboot.
    NoOp,
    /// `wardnetd-pending.tar.gz` was verified, `wardnetd` extracted, and
    /// renamed into `<live>` with the previous binary preserved at
    /// `<live>.old`.
    Swapped,
    /// `wardnetd-rollback.request` was honoured: `<live>.old` renamed
    /// back over `<live>`. No-op (logged but successful) if no `.old`
    /// is present — the operator may have requested rollback against
    /// a fresh install.
    RolledBack,
    /// Something went wrong — signature verification failed, the tarball
    /// did not contain a `wardnetd` entry, or a filesystem operation
    /// errored. The error chain carries the failing path / reason.
    Failed(anyhow::Error),
}

/// Run the verify-and-swap step. Called at the top of [`crate::Runner::run`].
///
/// Order of operations:
///   1. If a rollback request marker is present, honour it and return
///      (the operator wants the previous binary; ignoring a coincident
///      pending tarball is the safe choice).
///   2. If a pending tarball is staged, re-verify its signature and
///      replace `<live>` with the embedded `wardnetd`.
///   3. Otherwise no-op.
#[must_use]
pub fn run(paths: &SwapPaths, public_key: &str) -> SwapOutcome {
    if paths.rollback_request.exists() {
        return match perform_rollback(paths) {
            Ok(()) => SwapOutcome::RolledBack,
            Err(e) => SwapOutcome::Failed(e),
        };
    }

    let Some((tarball, signature)) =
        verify::read_artifacts(&paths.pending_tarball, &paths.pending_signature)
    else {
        return SwapOutcome::NoOp;
    };

    if let Err(e) = verify::verify(public_key, &tarball, &signature) {
        return SwapOutcome::Failed(e.context("pending tarball signature verification failed"));
    }

    match perform_swap(paths, &tarball) {
        Ok(()) => SwapOutcome::Swapped,
        Err(e) => SwapOutcome::Failed(e),
    }
}

fn perform_swap(paths: &SwapPaths, tarball: &[u8]) -> anyhow::Result<()> {
    let bytes = pluck_wardnetd(tarball).context("extract wardnetd from pending tarball")?;

    let parent = paths.live_binary.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "live binary path has no parent: {}",
            paths.live_binary.display()
        )
    })?;
    // Same dir as <live> so the final rename is intra-filesystem and
    // therefore atomic. Keeping it adjacent to <live> also means the
    // root-owned parent dir's permissions gate writes — wardnet user
    // can never poison this staging file post-verify.
    let staged = parent.join("wardnetd.swap-staging");
    write_executable(&staged, &bytes)?;

    let old = paths.old_binary();
    if paths.live_binary.exists() {
        if old.exists() {
            std::fs::remove_file(&old).with_context(|| format!("remove_file {}", old.display()))?;
        }
        std::fs::rename(&paths.live_binary, &old).with_context(|| {
            format!(
                "rename {} -> {}",
                paths.live_binary.display(),
                old.display()
            )
        })?;
    }
    std::fs::rename(&staged, &paths.live_binary).with_context(|| {
        format!(
            "rename {} -> {}",
            staged.display(),
            paths.live_binary.display()
        )
    })?;

    // The swap succeeded; the daemon-side staged artefacts have served
    // their purpose. Removing them prevents a stale apply on the next
    // boot if something else triggers the runner before another
    // auto-update writes a fresh pair.
    let _ = std::fs::remove_file(&paths.pending_tarball);
    let _ = std::fs::remove_file(&paths.pending_signature);

    Ok(())
}

fn perform_rollback(paths: &SwapPaths) -> anyhow::Result<()> {
    let old = paths.old_binary();
    let result = if old.exists() {
        if paths.live_binary.exists() {
            std::fs::remove_file(&paths.live_binary)
                .with_context(|| format!("remove_file {}", paths.live_binary.display()))?;
        }
        std::fs::rename(&old, &paths.live_binary).with_context(|| {
            format!(
                "rename {} -> {}",
                old.display(),
                paths.live_binary.display()
            )
        })
    } else {
        tracing::warn!(
            old = %old.display(),
            "rollback requested but no previous binary at {old}; clearing request and exiting clean",
            old = old.display(),
        );
        Ok(())
    };

    // Clear the request regardless of outcome — leaving it in place
    // would loop the runner on every boot. A failure here is logged
    // but doesn't block subsequent retries (daemon would re-write
    // the marker if the operator requests rollback again).
    let _ = std::fs::remove_file(&paths.rollback_request);

    result
}

/// Single pass over the gzip-tar tarball; returns the bytes of the
/// `wardnetd` entry. Anything else is ignored.
fn pluck_wardnetd(tarball: &[u8]) -> anyhow::Result<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(tarball);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries().context("read tarball entries")? {
        let mut entry = entry.context("read tarball entry header")?;
        let path = entry.path().context("entry path")?.into_owned();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if name == ARCHIVE_NAME_DAEMON {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .context("read wardnetd entry body")?;
            return Ok(buf);
        }
    }
    Err(anyhow::anyhow!(
        "tarball did not contain a `wardnetd` entry at any path"
    ))
}

fn write_executable(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;
    let mut out =
        std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    out.write_all(bytes)
        .with_context(|| format!("write {}", path.display()))?;
    drop(out);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .with_context(|| format!("metadata {}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)
            .with_context(|| format!("set_permissions {}", path.display()))?;
    }
    Ok(())
}
