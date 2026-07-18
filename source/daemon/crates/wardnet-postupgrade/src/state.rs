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
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing state file {}", path.display())),
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
fn write_root_only(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    // The creation mode does not apply when the file already exists —
    // a stale tmp from an interrupted earlier run keeps its old mode —
    // so tighten it explicitly either way.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
