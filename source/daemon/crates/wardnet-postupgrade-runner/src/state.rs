//! Best-effort state.json writer for verification failures.
//!
//! On a boot-blocking failure the runner records who/why/when into
//! the root-only state file so operators can diagnose without
//! scraping systemd journals. The migration runner
//! (`wardnet-postupgrade`) owns the rest of the state schema; this
//! module touches only the `last_verification_failure` and
//! `last_runner_failure` fields.

use std::path::Path;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Schema mirrors `wardnet-postupgrade::state::State`. Kept in sync
/// manually because the runner cannot depend on the migration runner
/// crate at runtime (the runner is the trust anchor; the migration
/// runner is the thing it execs). A dev-dependency enforces the
/// contract in tests instead.
///
/// `applied`/`failed` are `Vec<Value>` rather than bare `Value`:
/// entries pass through opaquely either way, but the `Vec` makes the
/// array shape part of the parse. The migration runner's typed schema
/// rejects an explicit JSON `null` for these fields, so a file
/// carrying one (written by older runner versions recovering from a
/// corrupt file) must fail deserialization here and take the
/// start-fresh branch below — which rewrites it as `[]` and unblocks
/// `State::load` on the next boot. Serialized `Vec::default()` is
/// `[]`, so the derived `Default` is safe to write back.
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    applied: Vec<serde_json::Value>,
    #[serde(default)]
    failed: Vec<serde_json::Value>,
    #[serde(default)]
    last_verification_failure: Option<VerificationFailure>,
    #[serde(default)]
    last_runner_failure: Option<RunnerFailure>,
}

#[derive(Debug, Serialize, Deserialize)]
struct VerificationFailure {
    error: String,
    at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RunnerFailure {
    stage: String,
    error: String,
    at: DateTime<Utc>,
}

/// Record a signature-verification failure into `state.json`,
/// preserving any `applied`/`failed` arrays already written by the
/// migration runner. Atomic write-then-rename, root-only file.
pub fn record_verification_failure(
    state_path: &Path,
    error_message: &str,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    update_state(state_path, |state| {
        state.last_verification_failure = Some(VerificationFailure {
            error: error_message.to_owned(),
            at: now,
        });
    })
}

/// Record a non-verification runner failure (artifact read, binary
/// swap, exec) into `state.json`, so a boot-blocked daemon is
/// diagnosable from the state file alongside verification failures.
pub fn record_runner_failure(
    state_path: &Path,
    stage: &str,
    error_message: &str,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    update_state(state_path, |state| {
        state.last_runner_failure = Some(RunnerFailure {
            stage: stage.to_owned(),
            error: error_message.to_owned(),
            at: now,
        });
    })
}

/// Load-or-start-fresh, apply `mutate`, write back atomically.
fn update_state(state_path: &Path, mutate: impl FnOnce(&mut State)) -> anyhow::Result<()> {
    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create state directory {}", parent.display()))?;
    }

    let mut state: State = match std::fs::read(state_path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            // Corrupt or schema-invalid state.json (truncated
            // mid-write, or null arrays from an older runner). Losing
            // the applied/failed history is the lesser evil versus
            // refusing to record the verification failure at all.
            let path = state_path.display().to_string();
            tracing::warn!(
                error = %e,
                %path,
                "existing state at {path} is unusable; starting fresh: {e}",
            );
            State::default()
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
        Err(e) => {
            return Err(anyhow::Error::from(e).context(format!(
                "could not read existing state at {}",
                state_path.display()
            )));
        }
    };
    mutate(&mut state);

    let serialized = serde_json::to_vec_pretty(&state).context("serialize state.json")?;
    let tmp = state_path.with_extension("json.tmp");
    write_root_only(&tmp, &serialized)
        .with_context(|| format!("write {} failed", tmp.display()))?;
    std::fs::rename(&tmp, state_path).with_context(|| {
        format!(
            "rename {} -> {} failed",
            tmp.display(),
            state_path.display()
        )
    })?;
    Ok(())
}

/// Write `bytes` to `path` with mode 0600. The rename that follows
/// carries the tmp file's permissions to state.json, so this is what
/// upholds the file's documented root-only mode — a plain
/// `std::fs::write` would leave it world-readable (0644).
///
/// Mirrored in `wardnet-postupgrade/src/state.rs` (same manual-sync
/// rule as the schema above) — apply changes to both copies.
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
    // Flush to disk before the caller renames over state.json, so a
    // power loss right after the rename cannot replay a zero-length
    // file where the old state used to be.
    file.sync_all()?;
    Ok(())
}
