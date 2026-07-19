//! `state.json` schema + atomic read/write.
//!
//! Path: `/var/lib/wardnet-postupgrade/state.json`. Owner: `root:root`,
//! mode `0600`. Intentionally outside `wardnetd.service`'s
//! `ReadWritePaths` so the unprivileged `wardnet` user cannot rewrite
//! it to mark a `Required` failure as `applied` and bypass the
//! daemon-startup gate.
//!
//! State is append-only from the runner's perspective: applied and
//! failed entries are preserved across boots, even if the underlying
//! migration is later removed from the in-binary `migrations()`
//! list. That keeps a stable audit trail an operator can inspect.

use std::path::Path;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Default location of `state.json` on a real Pi. Tests pass tempdir
/// paths instead.
pub const DEFAULT_STATE_PATH: &str = "/var/lib/wardnet-postupgrade/state.json";

/// Persisted state. New optional fields land at the end so the
/// runner can read state files written by older versions of itself.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub applied: Vec<AppliedEntry>,
    pub failed: Vec<FailedEntry>,
    /// Recorded by `wardnet-postupgrade-runner` (the trust anchor)
    /// when signature verification fails. The migration runner does
    /// not touch this field — it only ever runs after verification
    /// has succeeded.
    pub last_verification_failure: Option<VerificationFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedEntry {
    pub id: String,
    pub applied_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedEntry {
    pub id: String,
    pub error: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationFailure {
    pub error: String,
    pub at: DateTime<Utc>,
}

impl State {
    /// Read state from disk. Returns `Default::default()` when the
    /// file is absent — first run on a fresh host.
    ///
    /// Also reconciles the file's mode back to the documented 0600:
    /// older releases wrote state.json world-readable (0644), and on a
    /// healthy host nothing ever rewrites the file, so load — which
    /// runs on every boot — is the one place that reliably sees it.
    /// Best-effort: a chmod failure must not block migrations.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(path)
                        && meta.permissions().mode() & 0o077 != 0
                    {
                        let _ =
                            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
                    }
                }
                serde_json::from_slice(&bytes)
                    .with_context(|| format!("parsing state file {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => {
                Err(anyhow::Error::from(e)
                    .context(format!("reading state file {}", path.display())))
            }
        }
    }

    /// Atomic write-then-rename. Caller is responsible for ensuring
    /// the parent directory exists with the right ownership/mode —
    /// install.sh creates `/var/lib/wardnet-postupgrade/`.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let bytes = serde_json::to_vec_pretty(self).context("serialize state")?;
        let tmp = path.with_extension("json.tmp");
        write_root_only(&tmp, &bytes).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
        Ok(())
    }

    /// Has the migration with this id ever been recorded as applied?
    #[must_use]
    pub fn is_applied(&self, id: &str) -> bool {
        self.applied.iter().any(|e| e.id == id)
    }
}

/// Write `bytes` to `path` with mode 0600. The rename in `save`
/// carries the tmp file's permissions to state.json, so this is what
/// upholds the root-only mode documented at the top of this module —
/// a plain `std::fs::write` would leave it world-readable (0644).
///
/// Mirrored in `wardnet-postupgrade-runner/src/state.rs` (same
/// manual-sync rule as the schema) — apply changes to both copies.
fn write_root_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    // Drop any stale tmp from an interrupted earlier run before
    // creating a fresh inode (`create_new`). Reusing the old inode
    // would keep its old (possibly world-readable) mode, and a chmod
    // cannot revoke an fd someone already holds on it — an unlinked
    // inode's readers only ever see the bytes that were theirs.
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    // The creation mode is masked by umask (only ever tighter than
    // 0600); normalize to exactly the documented mode while the file
    // is still empty.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(bytes)?;
    // Flush to disk before `save` renames over state.json, so a power
    // loss right after the rename cannot replay a zero-length file
    // where the old state used to be.
    file.sync_all()?;
    Ok(())
}
