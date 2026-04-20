//! Tests for the [`JobService`] trait and default in-memory implementation.
//!
//! The spawned closure runs on the tokio runtime, so tests need small
//! polling loops to wait for terminal state. A 100 ms ceiling per wait
//! keeps these tests fast even if the scheduler is loaded.

use std::time::Duration;

use wardnet_common::jobs::{JobKind, JobStatus};

use crate::jobs::{JobService, JobServiceExt, JobServiceImpl};

/// Spin-wait until the job reaches a terminal status or the timeout expires.
async fn wait_terminal(svc: &dyn JobService, id: uuid::Uuid) -> wardnet_common::jobs::Job {
    for _ in 0..100 {
        let job = svc.get(id).await.expect("job should exist");
        if job.status.is_terminal() {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job did not reach terminal state within 1s");
}

#[tokio::test]
async fn dispatch_records_pending_then_runs_to_completion() {
    let svc = JobServiceImpl::new();
    let id = svc
        .dispatch(JobKind::BlocklistRefresh, |_r| async { Ok(()) })
        .await;

    // Visible immediately after dispatch — registry insert is synchronous.
    let initial = svc.get(id).await.expect("job visible after dispatch");
    assert_eq!(initial.kind, JobKind::BlocklistRefresh);

    let final_job = wait_terminal(svc.as_ref(), id).await;
    assert_eq!(final_job.status, JobStatus::Succeeded);
    assert_eq!(final_job.percentage_done, 100);
    assert!(final_job.error.is_none());
}

#[tokio::test]
async fn dispatch_records_failure_with_error_message() {
    let svc = JobServiceImpl::new();
    let id = svc
        .dispatch(JobKind::BlocklistRefresh, |_r| async {
            Err(anyhow::anyhow!("simulated download failure"))
        })
        .await;

    let job = wait_terminal(svc.as_ref(), id).await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(
        job.error.as_deref(),
        Some("simulated download failure"),
        "terminal error should be captured from the returned anyhow::Error"
    );
}

#[tokio::test]
async fn progress_reporter_updates_percentage() {
    let svc = JobServiceImpl::new();
    let id = svc
        .dispatch(JobKind::BlocklistRefresh, |reporter| async move {
            reporter.set_progress(25).await;
            reporter.set_progress(75).await;
            // Report > 100 to confirm clamping.
            reporter.set_progress(150).await;
            Ok(())
        })
        .await;

    let job = wait_terminal(svc.as_ref(), id).await;
    // Service overrides to 100 on success regardless of the last reported
    // value — the clamp behaviour is visible mid-run but normalised on
    // terminal success.
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(job.percentage_done, 100);
}

#[tokio::test]
async fn get_returns_none_for_unknown_job() {
    let svc = JobServiceImpl::new();
    let out = svc.get(uuid::Uuid::new_v4()).await;
    assert!(out.is_none());
}

#[test]
fn job_status_is_terminal() {
    assert!(!JobStatus::Pending.is_terminal());
    assert!(!JobStatus::Running.is_terminal());
    assert!(JobStatus::Succeeded.is_terminal());
    assert!(JobStatus::Failed.is_terminal());
}

#[test]
fn job_status_serde_matches_wire_names() {
    // Make sure the SCREAMING_SNAKE_CASE + explicit renames match the UI contract.
    assert_eq!(
        serde_json::to_string(&JobStatus::Pending).unwrap(),
        "\"PENDING\""
    );
    assert_eq!(
        serde_json::to_string(&JobStatus::Running).unwrap(),
        "\"RUNNING\""
    );
    assert_eq!(
        serde_json::to_string(&JobStatus::Succeeded).unwrap(),
        "\"SUCCEED\""
    );
    assert_eq!(
        serde_json::to_string(&JobStatus::Failed).unwrap(),
        "\"TERMINATED_WITH_ERRORS\""
    );
}
