//! Iterate the migration list, apply unapplied migrations, persist
//! state. Runner logic is a free function so tests can drive it
//! against a synthetic migration list and tempdir state path.

use std::path::Path;

use chrono::Utc;

use crate::migrations::{Migration, Severity};
use crate::state::{AppliedEntry, FailedEntry, State};

/// Outcome of a `run` invocation. Mapped to a process exit code by
/// `main.rs` (`Ok` → 0; `RequiredFailed` → 1).
#[derive(Debug)]
pub enum RunOutcome {
    /// Either nothing to do, or all `Required` migrations succeeded
    /// (with possibly some `Optional` failures recorded).
    Ok(RunReport),
    /// At least one `Required` migration failed. wardnetd must not
    /// start. The failed entry is already persisted to state.json.
    RequiredFailed(RunReport),
}

/// Per-run summary, suitable for a single INFO log line. Counts both
/// what was attempted in this run and what was already applied from a
/// previous boot.
#[derive(Debug, Default)]
pub struct RunReport {
    pub already_applied: usize,
    pub newly_applied: usize,
    pub optional_failures: usize,
    pub required_failures: usize,
    pub orphan_state_entries: usize,
}

/// Apply every migration not already in `state.applied[]`. Persists
/// state after each migration so a power loss leaves the partial
/// progress on disk.
pub fn run(state_path: &Path, migrations: &[Migration]) -> anyhow::Result<RunOutcome> {
    let mut state = State::load(state_path)?;
    let mut report = RunReport::default();

    // Audit trail integrity: warn (do not delete) on state entries
    // whose migration is no longer compiled in. Removed migrations
    // must never re-run.
    for entry in &state.applied {
        if !migrations.iter().any(|m| m.id == entry.id) {
            report.orphan_state_entries += 1;
            tracing::warn!(
                migration = %entry.id,
                "applied state entry has no matching migration in this binary: id={migration}",
                migration = entry.id,
            );
        }
    }

    for migration in migrations {
        if state.is_applied(migration.id) {
            report.already_applied += 1;
            continue;
        }

        tracing::info!(
            migration = %migration.id,
            severity = ?migration.severity,
            "applying migration: id={migration}",
            migration = migration.id,
        );

        match (migration.up)() {
            Ok(()) => {
                state.applied.push(AppliedEntry {
                    id: migration.id.to_owned(),
                    applied_at: Utc::now(),
                });
                state.save(state_path)?;
                report.newly_applied += 1;
                tracing::info!(
                    migration = %migration.id,
                    "migration applied: id={migration}",
                    migration = migration.id,
                );
            }
            Err(e) => {
                let entry = FailedEntry {
                    id: migration.id.to_owned(),
                    error: format!("{e:#}"),
                    at: Utc::now(),
                };
                state.failed.push(entry);
                state.save(state_path)?;

                match migration.severity {
                    Severity::Required => {
                        tracing::error!(
                            migration = %migration.id,
                            error = %e,
                            "Required migration failed: id={migration}, error={e}",
                            migration = migration.id,
                        );
                        report.required_failures += 1;
                        return Ok(RunOutcome::RequiredFailed(report));
                    }
                    Severity::Optional => {
                        tracing::warn!(
                            migration = %migration.id,
                            error = %e,
                            "Optional migration failed: id={migration}, error={e}",
                            migration = migration.id,
                        );
                        report.optional_failures += 1;
                    }
                }
            }
        }
    }

    // Always persist final state — even when nothing changed in
    // this run — so operators (and the e2e harness) have a stable
    // "I ran and exited cleanly" marker. On a fresh host with an
    // empty migration list, this writes the canonical empty state
    // file the next boot's parse will read back unchanged.
    state.save(state_path)?;

    Ok(RunOutcome::Ok(report))
}
