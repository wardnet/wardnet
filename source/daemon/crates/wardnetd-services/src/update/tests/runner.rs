use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use wardnet_common::api::{
    InstallUpdateRequest, InstallUpdateResponse, RollbackResponse, UpdateCheckResponse,
    UpdateConfigRequest, UpdateConfigResponse, UpdateHistoryResponse, UpdateStatusResponse,
};
use wardnet_common::update::{InstallHandle, InstallPhase, UpdateChannel, UpdateStatus};

use crate::error::AppError;
use crate::update::runner::UpdateRunner;
use crate::update::service::UpdateService;

struct CountingService {
    checks: AtomicU32,
    auto_installs: AtomicU32,
}

#[async_trait]
impl UpdateService for CountingService {
    async fn status(&self) -> Result<UpdateStatusResponse, AppError> {
        Ok(UpdateStatusResponse {
            status: UpdateStatus {
                current_version: "0.1.0".to_owned(),
                latest_version: None,
                update_available: false,
                auto_update_enabled: false,
                channel: UpdateChannel::Stable,
                last_check_at: None,
                last_install_at: None,
                install_phase: InstallPhase::Idle,
                pending_version: None,
                rollback_available: false,
            },
        })
    }
    async fn check(&self) -> Result<UpdateCheckResponse, AppError> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        Ok(UpdateCheckResponse {
            status: self.status().await?.status,
        })
    }
    async fn install(&self, _req: InstallUpdateRequest) -> Result<InstallUpdateResponse, AppError> {
        unimplemented!()
    }
    async fn rollback(&self) -> Result<RollbackResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: UpdateConfigRequest,
    ) -> Result<UpdateConfigResponse, AppError> {
        unimplemented!()
    }
    async fn history(&self, _limit: u32) -> Result<UpdateHistoryResponse, AppError> {
        unimplemented!()
    }
    async fn auto_install_if_due(&self) -> Result<Option<InstallHandle>, AppError> {
        self.auto_installs.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_performs_initial_check_on_start() {
    let service = Arc::new(CountingService {
        checks: AtomicU32::new(0),
        auto_installs: AtomicU32::new(0),
    });
    let service_dyn: Arc<dyn UpdateService> = service.clone();
    let parent = tracing::Span::none();
    let runner = UpdateRunner::start(service_dyn, Duration::from_hours(1), &parent);

    // Wait for the initial check.
    for _ in 0..40 {
        if service.checks.load(Ordering::SeqCst) > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(service.checks.load(Ordering::SeqCst) >= 1);
    assert!(service.auto_installs.load(Ordering::SeqCst) >= 1);

    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_shutdown_is_responsive() {
    let service = Arc::new(CountingService {
        checks: AtomicU32::new(0),
        auto_installs: AtomicU32::new(0),
    });
    let service_dyn: Arc<dyn UpdateService> = service;
    let parent = tracing::Span::none();
    let runner = UpdateRunner::start(service_dyn, Duration::from_hours(1), &parent);

    tokio::time::timeout(Duration::from_secs(5), runner.shutdown())
        .await
        .expect("runner should shut down promptly");
}
