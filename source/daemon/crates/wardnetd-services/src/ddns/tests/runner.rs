//! Runner tests: the runner ticks (refreshing once immediately) and shuts down
//! cleanly. The "inert when unconfigured" property is covered at the service
//! layer (`refresh_is_noop_when_unconfigured`); here we use a counting mock
//! service to confirm the runner drives it and stops on cancellation.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;

use crate::ddns::runner::DdnsUpdateRunner;
use crate::ddns::{DdnsRegistration, DdnsService, DdnsStatus};
use crate::error::AppError;

#[derive(Default)]
struct CountingDdns {
    refreshes: AtomicUsize,
}

#[async_trait]
impl DdnsService for CountingDdns {
    async fn register_with_bridge(&self, _name: String) -> Result<DdnsRegistration, AppError> {
        unreachable!("not called by the runner")
    }
    async fn check_name_available(&self, _name: String) -> Result<bool, AppError> {
        unreachable!("not called by the runner")
    }
    async fn configure_cloudflare(
        &self,
        _token: String,
        _domain: String,
    ) -> Result<DdnsRegistration, AppError> {
        unreachable!("not called by the runner")
    }
    async fn refresh_public_ip(&self) -> Result<Option<Ipv4Addr>, AppError> {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }
    async fn status(&self) -> Result<DdnsStatus, AppError> {
        unreachable!("not called by the runner")
    }
    async fn teardown(&self) -> Result<(), AppError> {
        unreachable!("not called by the runner")
    }
    async fn resolution_check(
        &self,
    ) -> Result<wardnet_common::api::DdnsResolutionCheckResponse, AppError> {
        unreachable!("not called by the runner")
    }
    async fn set_acme_challenge(&self, _values: &[String]) -> Result<(), AppError> {
        unreachable!("not called by the runner")
    }
    async fn clear_acme_challenge(&self) -> Result<(), AppError> {
        unreachable!("not called by the runner")
    }
}

#[tokio::test]
async fn runner_refreshes_then_shuts_down() {
    let mock = Arc::new(CountingDdns::default());
    let runner = DdnsUpdateRunner::start(mock.clone(), &tracing::Span::none());

    // The first interval tick resolves immediately → one refresh.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    runner.shutdown().await;

    assert!(mock.refreshes.load(Ordering::SeqCst) >= 1);
}
