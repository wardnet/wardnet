//! Privileged post-upgrade migration runner.
//!
//! Compiled into a small standalone binary that ships inside each
//! release tarball, signed with the same minisign key as `wardnetd`.
//! The trust anchor (`wardnet-postupgrade-runner`) verifies the
//! signature at every boot before exec'ing this binary as root.
//!
//! At each invocation the binary reads its persistent state from
//! `/var/lib/wardnet-postupgrade/state.json`, iterates the
//! compile-time migration list, and runs any migration that hasn't
//! been recorded as `applied`. State writes are atomic
//! (write-tmp-then-rename) so a power loss mid-migration leaves the
//! previous state intact and the migration re-runs on next boot.
//!
//! Migration functions must be **idempotent reconcilers** — running
//! the same migration twice must be a no-op once the desired state
//! is reached.

pub mod migrations;
pub mod runner;
pub mod state;
pub mod up;

pub use migrations::{Migration, Severity, migrations};
pub use runner::{RunOutcome, RunReport, run};
pub use state::{AppliedEntry, DEFAULT_STATE_PATH, FailedEntry, State};

#[cfg(test)]
mod tests;
