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
use crate::entitlement::Entitlement;
use crate::error::AppError;

#[derive(Default)]
struct CountingDdns {
    refreshes: AtomicUsize,
    probes: AtomicUsize,
}

#[async_trait]
impl DdnsService for CountingDdns {
    async fn request_enrollment_code(&self, _email: String) -> Result<(), AppError> {
        unreachable!("not called by the runner")
    }
    async fn enroll(&self, _code: String) -> Result<(), AppError> {
        unreachable!("not called by the runner")
    }
    async fn check_slug(&self, _slug: String) -> Result<bool, AppError> {
        unreachable!("not called by the runner")
    }
    async fn register_network(
        &self,
        _slug: String,
        _display_name: Option<String>,
    ) -> Result<DdnsRegistration, AppError> {
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
    async fn probe_entitlement(&self) -> Result<(), AppError> {
        self.probes.fetch_add(1, Ordering::SeqCst);
        Ok(())
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
    let runner =
        DdnsUpdateRunner::start(mock.clone(), Entitlement::shared(), &tracing::Span::none());

    // The first interval tick resolves immediately → one refresh.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    runner.shutdown().await;

    assert!(mock.refreshes.load(Ordering::SeqCst) >= 1);
    assert_eq!(
        mock.probes.load(Ordering::SeqCst),
        0,
        "an entitled runner never probes — it refreshes"
    );
}

#[tokio::test]
async fn runner_probes_instead_of_refreshing_while_suspended() {
    let mock = Arc::new(CountingDdns::default());
    let entitlement = Entitlement::shared();
    entitlement.suspend();
    let runner = DdnsUpdateRunner::start(mock.clone(), entitlement, &tracing::Span::none());

    // The first interval tick resolves immediately → one entitlement re-probe,
    // and crucially no heavier refresh (which would `403` while suspended).
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    runner.shutdown().await;

    assert!(mock.probes.load(Ordering::SeqCst) >= 1);
    assert_eq!(
        mock.refreshes.load(Ordering::SeqCst),
        0,
        "a suspended runner re-probes entitlement, never publishes"
    );
}
