//! Runtime for the privileged post-upgrade migration runner.
//!
//! The runner is the trust anchor of the post-upgrade pipeline. It is
//! installed by `install.sh` to `/usr/local/libexec/wardnet/runner/` as
//! `root:root 0755`, lives outside `wardnetd.service`'s
//! `ReadWritePaths`, and so cannot be replaced by the unprivileged
//! `wardnet` user that auto-update runs as. Its embedded public key is
//! the same minisign key that signs release tarballs.
//!
//! At every boot, systemd starts `wardnet-postupgrade.service` before
//! `wardnetd.service`. The service `ExecStart` is the runner, which:
//!   1. Reads the wardnet-writable payload + signature from
//!      `/var/lib/wardnet/postupgrade/`.
//!   2. Verifies the signature against the embedded public key.
//!   3. Writes the payload bytes into a `memfd_create` anonymous file.
//!   4. `fexecve`s the memfd as root, replacing this process with the
//!      `wardnet-postupgrade` binary.
//!
//! Holding the verified bytes in a memfd (never on disk) closes the
//! TOCTOU window between verify and exec — the wardnet user cannot
//! swap the payload file between `read()` and `exec()` because the
//! exec'd image is already in kernel memory.

pub mod exec;
pub mod state;
pub mod swap;
pub mod verify;

use std::path::PathBuf;

/// Default location of the wardnet-writable signed payload.
pub const DEFAULT_PAYLOAD_PATH: &str = "/var/lib/wardnet/postupgrade/wardnet-postupgrade.bin";
/// Default location of the wardnet-writable detached minisign signature.
pub const DEFAULT_SIGNATURE_PATH: &str = "/var/lib/wardnet/postupgrade/wardnet-postupgrade.minisig";
/// Default location of the root-only state file.
///
/// Intentionally a sibling of `/var/lib/wardnet/` so the wardnet user
/// (covered by `wardnetd.service`'s `ReadWritePaths=/var/lib/wardnet`)
/// cannot rewrite this file to mark a `Required` failure as `applied`
/// and bypass the daemon-startup gate.
pub const DEFAULT_STATE_PATH: &str = "/var/lib/wardnet-postupgrade/state.json";
/// Default location of the live wardnetd binary swapped into place by
/// the privileged swap step. Mirrors the daemon's
/// `config.update.live_binary_path` default.
pub const DEFAULT_LIVE_BINARY_PATH: &str = "/usr/local/bin/wardnetd";

/// Inputs the runner needs to verify and exec a payload. Mostly fixed
/// at compile time; `Runner::with_default_paths` wires the production
/// paths and tests construct alternate paths in tempdirs.
pub struct Runner {
    pub payload_path: PathBuf,
    pub signature_path: PathBuf,
    pub state_path: PathBuf,
    pub swap_paths: swap::SwapPaths,
    pub public_key: &'static str,
}

/// What the runner did. Mapped to a process exit code by `main.rs`.
#[derive(Debug)]
pub enum RunOutcome {
    /// Either the payload or its detached signature is missing.
    /// The runner logs INFO and exits 0 — there is nothing to gate
    /// daemon startup on. install.sh always places initial artifacts,
    /// so this branch is reached only on a hand-deleted file or an
    /// ancient tarball that predates the framework.
    NoPayload,
    /// The payload or signature exists but could not be read (payload
    /// path is a directory, permission error, failing disk). Distinct
    /// from [`RunOutcome::NoPayload`]: something *is* staged, so
    /// pretending nothing happened would silently skip an update. The
    /// runner records the failure into `state.json`, logs ERROR, and
    /// exits nonzero.
    ArtifactReadFailed(anyhow::Error),
    /// Signature verification failed. The runner logs ERROR, records
    /// the failure into `state.json`, and exits nonzero.
    VerifyFailed(anyhow::Error),
    /// `memfd_create` or `fexecve` failed. Successful exec replaces
    /// the process and never returns this variant. Very rare in
    /// practice (kernel out of memory, seccomp policy mismatch). The
    /// failure is recorded into `state.json`.
    ExecFailed(anyhow::Error),
    /// Privileged swap of `<live>` could not be completed (signature
    /// failure, missing wardnetd entry, fs error). The runner records
    /// the failure into `state.json` and exits nonzero so systemd
    /// refuses to start `wardnetd.service` against an unverified
    /// binary.
    SwapFailed(anyhow::Error),
}

impl Runner {
    /// Wire the production paths and embedded public key. Used by
    /// `main.rs`. Tests construct `Runner` directly with tempdir paths.
    #[must_use]
    pub fn with_default_paths(public_key: &'static str) -> Self {
        Self {
            payload_path: PathBuf::from(DEFAULT_PAYLOAD_PATH),
            signature_path: PathBuf::from(DEFAULT_SIGNATURE_PATH),
            state_path: PathBuf::from(DEFAULT_STATE_PATH),
            swap_paths: swap::SwapPaths::with_defaults(PathBuf::from(DEFAULT_LIVE_BINARY_PATH)),
            public_key,
        }
    }

    /// Verify the payload and exec it. Successful exec never returns;
    /// failure modes are mapped to [`RunOutcome`] variants. The caller
    /// (`main.rs`) translates the variant to a logger call + exit code.
    pub fn run(&self, args: &[std::ffi::CString]) -> RunOutcome {
        // Privileged daemon swap runs first — the migration runner the
        // exec branch invokes belongs to the new release, not the old
        // one. A swap failure aborts boot rather than running migrations
        // against a binary the trust anchor couldn't verify.
        match swap::run(&self.swap_paths, self.public_key) {
            swap::SwapOutcome::NoOp => {}
            swap::SwapOutcome::Swapped => {
                let live = self.swap_paths.live_binary.display().to_string();
                tracing::info!(%live, "wardnetd binary swapped into place at {live}");
            }
            swap::SwapOutcome::RolledBack => {
                let live = self.swap_paths.live_binary.display().to_string();
                tracing::info!(%live, "wardnetd rolled back to previous binary at {live}");
            }
            swap::SwapOutcome::Failed(e) => {
                self.record_runner_failure("swap", &e);
                return RunOutcome::SwapFailed(e);
            }
        }

        let (payload, signature) =
            match verify::read_artifacts(&self.payload_path, &self.signature_path) {
                Ok(Some(pair)) => pair,
                Ok(None) => return RunOutcome::NoPayload,
                Err(e) => {
                    self.record_runner_failure("artifact-read", &e);
                    return RunOutcome::ArtifactReadFailed(e);
                }
            };

        if let Err(e) = verify::verify(self.public_key, &payload, &signature) {
            // Record the failure best-effort; if state.json itself is
            // unwritable (read-only fs, missing dir) we still want the
            // ERROR log + nonzero exit to reach the journal.
            let now = chrono::Utc::now();
            if let Err(state_err) =
                state::record_verification_failure(&self.state_path, &format!("{e:#}"), now)
            {
                let state = self.state_path.display().to_string();
                tracing::warn!(
                    %state_err,
                    %state,
                    "could not record verification failure to {state}: {state_err}",
                );
            }
            return RunOutcome::VerifyFailed(e);
        }

        match exec::exec_memfd("wardnet-postupgrade", &payload, args) {
            Ok(never) => match never {},
            Err(e) => {
                self.record_runner_failure("exec", &e);
                RunOutcome::ExecFailed(e)
            }
        }
    }

    /// Best-effort mirror of the verification-failure recording above:
    /// every boot-blocking outcome should be visible in state.json,
    /// not only signature failures. Never fails the run — the ERROR
    /// log + nonzero exit must reach the journal regardless.
    fn record_runner_failure(&self, stage: &str, err: &anyhow::Error) {
        let now = chrono::Utc::now();
        if let Err(state_err) =
            state::record_runner_failure(&self.state_path, stage, &format!("{err:#}"), now)
        {
            let path = self.state_path.display().to_string();
            tracing::warn!(
                %state_err,
                %path,
                %stage,
                "could not record {stage} failure to {path}: {state_err}",
            );
        }
    }
}

#[cfg(test)]
mod tests;
