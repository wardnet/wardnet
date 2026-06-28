//! Service-level tests for the per-tunnel speed test (`start_speed_test` /
//! `list_speed_tests`). Reuses the tunnel test harness; the speed test job
//! runs in the background via the real in-memory `JobService`, so each test
//! dispatches then polls the job to completion.

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;
use wardnet_common::jobs::{Job, JobStatus};
use wardnet_common::tunnel::TunnelStatus;

use crate::TunnelService;
use crate::auth_context;
use crate::error::AppError;
use crate::jobs::JobService;
use crate::tunnel::interface::TunnelStats;
use wardnetd_data::repository::TunnelRepository;

use super::tunnel::{admin_ctx, build_harness, imported_tunnel_id};

/// Poll a job until it reaches a terminal state, or panic after a generous
/// budget (the mock throughput tester adds at most a few hundred ms).
async fn await_job(jobs: &Arc<dyn JobService>, job_id: Uuid) -> Job {
    for _ in 0..250 {
        if let Some(job) = jobs.get(job_id).await
            && job.status.is_terminal()
        {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("speed test job {job_id} did not terminate in time");
}

/// A fresh handshake so `await_fresh_handshake` clears immediately when the
/// job brings a tunnel up.
fn fresh_stats() -> TunnelStats {
    TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(chrono::Utc::now()),
    }
}

#[tokio::test]
async fn speed_test_persists_both_legs() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    // Already up: the job measures both legs without bringing it up.
    h.tunnels
        .update_status(&id.to_string(), "up")
        .await
        .unwrap();
    h.tunnel_iface.set_stats(fresh_stats());

    let resp = auth_context::with_context(admin_ctx(), h.svc.clone().start_speed_test(id))
        .await
        .expect("start_speed_test should dispatch");
    let job = await_job(&h.jobs, resp.job_id).await;
    assert_eq!(
        job.status,
        JobStatus::Succeeded,
        "job error: {:?}",
        job.error
    );

    // One row, both legs populated; mock reports direct 94 / tunnel 85 Mbps.
    assert_eq!(h.speed_test_repo.count(), 1);
    let rows = h.speed_test_repo.rows();
    let row = &rows[0];
    assert_eq!(row.tunnel_id, id.to_string());
    assert!((row.direct_throughput_mbps - 94.0).abs() < f64::EPSILON);
    assert!((row.tunnel_throughput_mbps - 85.0).abs() < f64::EPSILON);
    assert!(row.direct_latency_ms > 0.0);
    assert!(row.tunnel_latency_ms > 0.0);

    // Direct leg measured first (unbound), then the tunnel leg (bound).
    let calls = h.throughput_tester.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0], None, "direct leg should be unbound");
    assert!(calls[1].is_some(), "tunnel leg should bind the interface");

    // History surfaces the persisted run.
    let history = auth_context::with_context(admin_ctx(), h.svc.list_speed_tests(id))
        .await
        .unwrap();
    assert_eq!(history.results.len(), 1);
    assert_eq!(history.results[0].tunnel_id, id);
}

#[tokio::test]
async fn speed_test_brings_up_down_tunnel_then_tears_down() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await; // imported tunnels start Down
    h.tunnel_iface.set_stats(fresh_stats());

    let resp = auth_context::with_context(admin_ctx(), h.svc.clone().start_speed_test(id))
        .await
        .unwrap();
    let job = await_job(&h.jobs, resp.job_id).await;
    assert_eq!(
        job.status,
        JobStatus::Succeeded,
        "job error: {:?}",
        job.error
    );

    assert!(
        h.tunnel_iface.created_count() >= 1,
        "expected the tunnel to be brought up for the run"
    );
    assert!(
        h.tunnel_iface.torn_down_count() >= 1,
        "expected the tunnel to be torn back down after the run"
    );
    // Final state restored to Down.
    let tunnel = h
        .tunnels
        .find_by_id(&id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Down);
    assert_eq!(h.speed_test_repo.count(), 1);
}

#[tokio::test]
async fn speed_test_concurrent_run_returns_conflict() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    h.tunnels
        .update_status(&id.to_string(), "up")
        .await
        .unwrap();
    h.tunnel_iface.set_stats(fresh_stats());
    // Hold the first run's download open long enough to overlap.
    h.throughput_tester.set_delay(300);

    let first = auth_context::with_context(admin_ctx(), h.svc.clone().start_speed_test(id))
        .await
        .expect("first run dispatches");

    // The background job has acquired the in-flight slot; a second request
    // for the same tunnel must be rejected.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let second = auth_context::with_context(admin_ctx(), h.svc.clone().start_speed_test(id)).await;
    assert!(
        matches!(second, Err(AppError::Conflict(_))),
        "expected Conflict on concurrent run, got {second:?}"
    );

    // Drain the first job so the test doesn't leak a task.
    await_job(&h.jobs, first.job_id).await;
}

#[tokio::test]
async fn speed_test_leg_failure_fails_job() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    h.tunnels
        .update_status(&id.to_string(), "up")
        .await
        .unwrap();
    h.tunnel_iface.set_stats(fresh_stats());
    // The tunnel leg's download fails — the whole test must fail, no row.
    h.throughput_tester.set_fail_tunnel(true);

    let resp = auth_context::with_context(admin_ctx(), h.svc.clone().start_speed_test(id))
        .await
        .unwrap();
    let job = await_job(&h.jobs, resp.job_id).await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(
        h.speed_test_repo.count(),
        0,
        "a failed run must not persist a half-result"
    );
}
