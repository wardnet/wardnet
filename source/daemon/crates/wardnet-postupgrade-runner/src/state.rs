//! Best-effort state.json writer for verification failures.
//!
//! On verify failure the runner records who/why/when into the
//! root-only state file so operators can diagnose without scraping
//! systemd journals. The migration runner (`wardnet-postupgrade`)
//! owns the rest of the state schema; this module touches only the
//! `last_verification_failure` field.

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
}

#[derive(Debug, Serialize, Deserialize)]
struct VerificationFailure {
    error: String,
    at: DateTime<Utc>,
}

/// Append a verification-failure record to `state.json`, preserving
/// any `applied`/`failed` arrays already written by the migration
/// runner. Atomic write-then-rename, root-only file.
pub fn record_verification_failure(
    state_path: &Path,
    error_message: &str,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
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
    state.last_verification_failure = Some(VerificationFailure {
        error: error_message.to_owned(),
        at: now,
    });

    let serialized = serde_json::to_vec_pretty(&state).context("serialize state.json")?;
    let tmp = state_path.with_extension("json.tmp");
    std::fs::write(&tmp, &serialized).with_context(|| format!("write {} failed", tmp.display()))?;
    std::fs::rename(&tmp, state_path).with_context(|| {
        format!(
            "rename {} -> {} failed",
            tmp.display(),
            state_path.display()
        )
    })?;
    Ok(())
}
