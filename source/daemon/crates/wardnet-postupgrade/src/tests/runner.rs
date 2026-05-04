//! Tests for `runner::run` against synthetic migration lists.
//!
//! Migration `up` fns must match `fn() -> anyhow::Result<()>` and so
//! cannot capture closure state. Tests assert outcome via the report
//! and the on-disk state file (each test owns a tempdir state path),
//! never through process-global counters — cargo runs tests in
//! parallel and shared atomics race across tests.
//!
//! `panic_if_called` is used wherever a test needs to prove a
//! migration is *not* re-run; if it ever runs the test panics and
//! fails, regardless of test interleaving.
#![allow(clippy::unnecessary_wraps)]

use tempfile::TempDir;

use crate::migrations::{Migration, Severity};
use crate::runner::{RunOutcome, run};
use crate::state::State;

fn ok() -> anyhow::Result<()> {
    Ok(())
}
fn fail() -> anyhow::Result<()> {
    Err(anyhow::anyhow!("synthetic failure"))
}
fn panic_if_called() -> anyhow::Result<()> {
    panic!("migration up fn was called when it should have been skipped");
}

#[test]
fn empty_list_writes_default_state_and_returns_ok() {
    let dir = TempDir::new().unwrap();
    let state_path = dir.path().join("state.json");
    let outcome = run(&state_path, &[]).expect("run");
    assert!(matches!(outcome, RunOutcome::Ok(_)));
    let state = State::load(&state_path).unwrap();
    assert!(state.applied.is_empty());
    assert!(state.failed.is_empty());
}

#[test]
fn required_success_persists_applied_entry() {
    let dir = TempDir::new().unwrap();
    let state_path = dir.path().join("state.json");
    let migrations = [Migration {
        id: "0001_test_required_ok",
        severity: Severity::Required,
        up: ok,
    }];
    let outcome = run(&state_path, &migrations).expect("run");
    let RunOutcome::Ok(report) = outcome else {
        panic!("expected Ok, got {outcome:?}");
    };
    assert_eq!(report.newly_applied, 1);
    assert_eq!(report.already_applied, 0);

    let state = State::load(&state_path).unwrap();
    assert_eq!(state.applied.len(), 1);
    assert_eq!(state.applied[0].id, "0001_test_required_ok");
}

#[test]
fn required_failure_returns_required_failed_and_persists() {
    let dir = TempDir::new().unwrap();
    let state_path = dir.path().join("state.json");
    let migrations = [Migration {
        id: "0001_test_required_fail",
        severity: Severity::Required,
        up: fail,
    }];
    let outcome = run(&state_path, &migrations).expect("run");
    let RunOutcome::RequiredFailed(report) = outcome else {
        panic!("expected RequiredFailed, got {outcome:?}");
    };
    assert_eq!(report.required_failures, 1);
    assert_eq!(report.newly_applied, 0);

    let state = State::load(&state_path).unwrap();
    assert_eq!(state.failed.len(), 1);
    assert_eq!(state.failed[0].id, "0001_test_required_fail");
    assert!(state.applied.is_empty());
}

#[test]
fn optional_failure_continues_run_and_logs() {
    let dir = TempDir::new().unwrap();
    let state_path = dir.path().join("state.json");
    let migrations = [
        Migration {
            id: "0001_test_optional_fail",
            severity: Severity::Optional,
            up: fail,
        },
        Migration {
            id: "0002_test_required_ok_after",
            severity: Severity::Required,
            up: ok,
        },
    ];
    let outcome = run(&state_path, &migrations).expect("run");
    let RunOutcome::Ok(report) = outcome else {
        panic!("expected Ok, got {outcome:?}");
    };
    assert_eq!(report.optional_failures, 1);
    assert_eq!(report.newly_applied, 1);

    let state = State::load(&state_path).unwrap();
    assert_eq!(state.failed.len(), 1);
    assert_eq!(state.applied.len(), 1);
    assert_eq!(state.applied[0].id, "0002_test_required_ok_after");
}

#[test]
fn already_applied_migration_does_not_rerun() {
    let dir = TempDir::new().unwrap();
    let state_path = dir.path().join("state.json");

    // First run records the migration as applied.
    let first = [Migration {
        id: "0001_test_idempotent",
        severity: Severity::Required,
        up: ok,
    }];
    let _ = run(&state_path, &first).expect("first run");

    // Second run with a panic-on-call up fn proves the runner does
    // not re-execute. If the migration's up fn ever runs, the test
    // panics regardless of test interleaving.
    let second = [Migration {
        id: "0001_test_idempotent",
        severity: Severity::Required,
        up: panic_if_called,
    }];
    let outcome = run(&state_path, &second).expect("second run");
    let RunOutcome::Ok(report) = outcome else {
        panic!("expected Ok on rerun");
    };
    assert_eq!(report.already_applied, 1);
    assert_eq!(report.newly_applied, 0);

    // State file still records exactly one applied entry; no
    // duplicate appended.
    let state = State::load(&state_path).unwrap();
    assert_eq!(state.applied.len(), 1);
    assert_eq!(state.applied[0].id, "0001_test_idempotent");
}

#[test]
fn orphan_state_entry_is_warned_not_pruned() {
    let dir = TempDir::new().unwrap();
    let state_path = dir.path().join("state.json");
    // Pre-seed state with an entry whose id isn't in `migrations`
    // (simulates a migration removed from the in-binary list).
    let now = chrono::Utc::now();
    let pre = State {
        applied: vec![crate::state::AppliedEntry {
            id: "0099_legacy_removed".into(),
            applied_at: now,
        }],
        ..Default::default()
    };
    pre.save(&state_path).unwrap();

    let outcome = run(&state_path, &[]).expect("run");
    let RunOutcome::Ok(report) = outcome else {
        panic!("expected Ok");
    };
    assert_eq!(report.orphan_state_entries, 1);

    let state = State::load(&state_path).unwrap();
    assert_eq!(state.applied.len(), 1, "orphan entry must be preserved");
    assert_eq!(state.applied[0].id, "0099_legacy_removed");
}

#[test]
fn second_required_failure_after_first_success_persists_both() {
    let dir = TempDir::new().unwrap();
    let state_path = dir.path().join("state.json");
    let migrations = [
        Migration {
            id: "0001_first_ok",
            severity: Severity::Required,
            up: ok,
        },
        Migration {
            id: "0002_second_required_fail",
            severity: Severity::Required,
            up: fail,
        },
    ];
    let outcome = run(&state_path, &migrations).expect("run");
    let RunOutcome::RequiredFailed(report) = outcome else {
        panic!("expected RequiredFailed");
    };
    assert_eq!(report.newly_applied, 1);
    assert_eq!(report.required_failures, 1);

    let state = State::load(&state_path).unwrap();
    assert_eq!(state.applied.len(), 1);
    assert_eq!(state.failed.len(), 1);
    assert_eq!(state.applied[0].id, "0001_first_ok");
    assert_eq!(state.failed[0].id, "0002_second_required_fail");
}
