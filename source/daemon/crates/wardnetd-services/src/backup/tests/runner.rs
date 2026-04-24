//! Tests for [`BackupCleanupRunner`].
//!
//! The runner is a thin loop around `cleanup_old_snapshots`, so the
//! tests just verify that it calls the service on every tick and that
//! `shutdown` actually stops the loop. The backup state machine
//! itself is covered by the service tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use wardnet_common::api::{
    ApplyImportRequest, ApplyImportResponse, BackupStatusResponse, ExportBackupRequest,
    ListSnapshotsResponse, RestorePreviewResponse,
};

use crate::backup::runner::{BackupCleanupRunner, DEFAULT_SNAPSHOT_RETENTION};
use crate::backup::service::BackupService;
use crate::error::AppError;

/// Counts `cleanup_old_snapshots` calls; every other method panics so
/// tests fail loudly if the runner starts calling something it
/// shouldn't.
struct CountingService {
    cleanup_calls: AtomicU32,
}

impl CountingService {
    fn new() -> Self {
        Self {
            cleanup_calls: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl BackupService for CountingService {
    async fn status(&self) -> Result<BackupStatusResponse, AppError> {
        unimplemented!()
    }
    async fn export(&self, _req: ExportBackupRequest) -> Result<Vec<u8>, AppError> {
        unimplemented!()
    }
    async fn preview_import(
        &self,
        _bundle: Vec<u8>,
        _passphrase: String,
    ) -> Result<RestorePreviewResponse, AppError> {
        unimplemented!()
    }
    async fn apply_import(
        &self,
        _req: ApplyImportRequest,
    ) -> Result<ApplyImportResponse, AppError> {
        unimplemented!()
    }
    async fn list_snapshots(&self) -> Result<ListSnapshotsResponse, AppError> {
        unimplemented!()
    }
    async fn cleanup_old_snapshots(&self, _retain: Duration) -> Result<u32, AppError> {
        self.cleanup_calls.fetch_add(1, Ordering::SeqCst);
        Ok(0)
    }
}

#[tokio::test]
async fn runner_fires_cleanup_on_startup() {
    let svc = Arc::new(CountingService::new());
    let parent = tracing::info_span!("wardnetd-test");

    let runner = BackupCleanupRunner::start(
        svc.clone() as Arc<dyn BackupService>,
        Duration::from_hours(1),
        DEFAULT_SNAPSHOT_RETENTION,
        &parent,
    );

    // Let the initial tick complete. The runner calls cleanup once
    // immediately on start, then waits for the next interval.
    for _ in 0..50 {
        if svc.cleanup_calls.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(svc.cleanup_calls.load(Ordering::SeqCst), 1);

    runner.shutdown().await;
}

#[tokio::test]
async fn runner_shuts_down_cleanly_without_tick() {
    // Even if the operator shuts down immediately after start, before
    // the initial tick finishes, the runner must not hang.
    let svc = Arc::new(CountingService::new());
    let parent = tracing::info_span!("wardnetd-test");

    let runner = BackupCleanupRunner::start(
        svc.clone() as Arc<dyn BackupService>,
        Duration::from_hours(1),
        DEFAULT_SNAPSHOT_RETENTION,
        &parent,
    );

    runner.shutdown().await;
}

#[test]
fn default_retention_is_24h() {
    assert_eq!(DEFAULT_SNAPSHOT_RETENTION, Duration::from_hours(24));
}
