//! Binary apply step — stage a verified release tarball for the
//! privileged swap that `wardnet-postupgrade-runner` performs at the
//! next daemon restart.
//!
//! The daemon runs as the unprivileged `wardnet` user and cannot
//! rename a binary into `/usr/local/bin/` (the parent dir is
//! `root:root 0755`). The swap therefore happens out-of-process: this
//! trait stages the tarball + signature into a wardnet-writable
//! directory and the runner re-verifies + renames as root before
//! `wardnetd.service` starts. See the runner crate's `swap` module
//! for the consuming side.

use async_trait::async_trait;
use std::path::PathBuf;

/// Result of a successful staging step.
///
/// The actual filesystem swap has *not* happened yet when this is
/// returned — only the staging step is in this trait's scope. The
/// `previous_binary` path is the location where the runner will
/// preserve the old binary on its next run, so the daemon can record
/// it for the rollback API.
#[derive(Debug, Clone)]
pub struct SwapOutcome {
    /// Where the previously-live binary will be moved to (`<live>.old`)
    /// once the runner performs the swap on the next daemon restart.
    pub previous_binary: PathBuf,
}

/// Stages a verified release tarball and signals the runner to swap it.
///
/// `apply` writes the tarball + minisig to a wardnet-writable location;
/// `rollback` writes a request marker the runner picks up on next boot.
/// `rollback_available` checks whether `<live>.old` is present (the
/// runner produced one on a previous successful swap).
#[async_trait]
pub trait BinaryApplier: Send + Sync {
    /// Stage a verified tarball for the privileged swap. On success,
    /// the daemon should request a restart so `wardnet-postupgrade.service`
    /// runs the swap before `wardnetd.service` starts. `signature` is
    /// the detached minisign signature of `tarball`; the runner will
    /// re-verify it against the embedded public key.
    async fn apply(&self, tarball: &[u8], signature: &[u8]) -> anyhow::Result<SwapOutcome>;

    /// Request a rollback to `<live>.old`. Returns an error if no
    /// rollback target is present. Like `apply`, the actual rename is
    /// performed by the runner on the next daemon restart.
    async fn rollback(&self) -> anyhow::Result<()>;

    /// Whether a `<live>.old` binary is present that can be rolled
    /// back to.
    async fn rollback_available(&self) -> bool;
}
