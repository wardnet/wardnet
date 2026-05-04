//! `wardnet-postupgrade` entrypoint.
//!
//! Run by `wardnet-postupgrade-runner` (the trust anchor) via `fexecve`
//! at every boot, after signature verification. systemd captures
//! stdout/stderr via journald with `SyslogIdentifier=wardnet-postupgrade`.

use std::path::PathBuf;

use wardnet_postupgrade::{DEFAULT_STATE_PATH, RunOutcome, migrations, run};

fn main() {
    init_tracing();

    let state_path = PathBuf::from(DEFAULT_STATE_PATH);
    let migrations = migrations();

    match run(&state_path, migrations) {
        Ok(RunOutcome::Ok(report)) => {
            tracing::info!(
                already_applied = report.already_applied,
                newly_applied = report.newly_applied,
                optional_failures = report.optional_failures,
                orphan_state_entries = report.orphan_state_entries,
                "post-upgrade migrations complete: already_applied={already_applied}, newly_applied={newly_applied}, optional_failures={optional_failures}, orphan_state_entries={orphan_state_entries}",
                already_applied = report.already_applied,
                newly_applied = report.newly_applied,
                optional_failures = report.optional_failures,
                orphan_state_entries = report.orphan_state_entries,
            );
            std::process::exit(0);
        }
        Ok(RunOutcome::RequiredFailed(report)) => {
            tracing::error!(
                already_applied = report.already_applied,
                newly_applied = report.newly_applied,
                required_failures = report.required_failures,
                "post-upgrade migrations halted on Required failure: already_applied={already_applied}, newly_applied={newly_applied}, required_failures={required_failures}",
                already_applied = report.already_applied,
                newly_applied = report.newly_applied,
                required_failures = report.required_failures,
            );
            std::process::exit(1);
        }
        Err(e) => {
            tracing::error!(
                error = %e,
                state = %state_path.display(),
                "post-upgrade runner aborted before iterating migrations: {e}",
            );
            std::process::exit(2);
        }
    }
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_target(false)
        .without_time()
        .try_init();
}
