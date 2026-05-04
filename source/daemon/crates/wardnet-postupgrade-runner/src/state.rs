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
/// crate (the runner is the trust anchor; the migration runner is the
/// thing it execs).
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    applied: serde_json::Value,
    #[serde(default)]
    failed: serde_json::Value,
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
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => State {
            applied: serde_json::Value::Array(Vec::new()),
            failed: serde_json::Value::Array(Vec::new()),
            last_verification_failure: None,
        },
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
