//! Tests for [`SessionCleanupRunner`]. Drives the runner with a recording
//! mock [`AuthService`] and asserts it calls `cleanup_expired_sessions` on
//! its interval (under an admin context), skips the immediate first tick, and
//! stops on shutdown.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{WizardMode, WizardStep};

use crate::auth::SessionCleanupRunner;
use crate::auth::service::{LoginResult, WizardState};
use crate::error::AppError;
use crate::{AuthService, auth_context};

/// Mock service that records every `cleanup_expired_sessions` call and whether
/// it observed an admin auth context, and returns a configurable outcome. All
/// other trait methods are unreachable in these tests.
struct MockAuthService {
    calls: AtomicU64,
    /// Incremented only when the call ran under a valid admin context.
    admin_calls: AtomicU64,
    /// Row count returned by a successful `cleanup_expired_sessions`.
    return_count: u64,
    /// When true, `cleanup_expired_sessions` returns an error.
    fail: bool,
}

impl MockAuthService {
    /// Succeeds returning `0` deleted rows.
    fn new() -> Self {
        Self::with_outcome(0, false)
    }

    /// Succeeds returning `count` deleted rows (exercises the "deleted" log).
    fn returning(count: u64) -> Self {
        Self::with_outcome(count, false)
    }

    /// Fails every call (exercises the error log + loop resilience).
    fn failing() -> Self {
        Self::with_outcome(0, true)
    }

    fn with_outcome(return_count: u64, fail: bool) -> Self {
        Self {
            calls: AtomicU64::new(0),
            admin_calls: AtomicU64::new(0),
            return_count,
            fail,
        }
    }
}

#[async_trait]
impl AuthService for MockAuthService {
    async fn current_admin_username(&self) -> Result<String, AppError> {
        Ok("admin".to_owned())
    }
    async fn login(
        &self,
        _username: &str,
        _password: &str,
        _remember_me: bool,
    ) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn logout_session(&self, _token: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        unimplemented!()
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        unimplemented!()
    }
    async fn setup_admin(&self, _username: &str, _password: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        unimplemented!()
    }
    async fn wizard_state(&self) -> Result<WizardState, AppError> {
        unimplemented!()
    }
    async fn advance_wizard(
        &self,
        _to_step: WizardStep,
        _mode: Option<WizardMode>,
    ) -> Result<WizardState, AppError> {
        unimplemented!()
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        if auth_context::require_admin().is_ok() {
            self.admin_calls.fetch_add(1, Ordering::SeqCst);
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(AppError::Internal(anyhow::anyhow!(
                "simulated cleanup failure"
            )));
        }
        Ok(self.return_count)
    }
}

/// Poll `f` up to `tries` times with a short sleep between attempts, returning
/// as soon as it is true.
async fn wait_until(tries: u32, mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..tries {
        if f() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    f()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runs_cleanup_on_tick_under_admin_context() {
    let service = Arc::new(MockAuthService::new());
    let parent = tracing::Span::none();

    let runner = SessionCleanupRunner::start_with_interval(
        Arc::clone(&service) as Arc<dyn AuthService>,
        Duration::from_millis(10),
        &parent,
    );

    let fired = wait_until(50, || service.calls.load(Ordering::SeqCst) > 0).await;
    assert!(fired, "cleanup should have run at least once");
    assert!(
        service.admin_calls.load(Ordering::SeqCst) > 0,
        "cleanup should run under an admin auth context"
    );

    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skips_immediate_startup_tick() {
    let service = Arc::new(MockAuthService::new());
    let parent = tracing::Span::none();

    // Long interval: the only tick that could fire quickly is the immediate
    // first one, which the runner consumes. So no call should be observed.
    let runner = SessionCleanupRunner::start_with_interval(
        Arc::clone(&service) as Arc<dyn AuthService>,
        Duration::from_hours(1),
        &parent,
    );

    // Give the task a chance to run to its first `ticker.tick().await`.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        service.calls.load(Ordering::SeqCst),
        0,
        "the immediate first tick must be skipped"
    );

    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_stops_the_loop() {
    let service = Arc::new(MockAuthService::new());
    let parent = tracing::Span::none();

    let runner = SessionCleanupRunner::start_with_interval(
        Arc::clone(&service) as Arc<dyn AuthService>,
        Duration::from_millis(10),
        &parent,
    );

    assert!(
        wait_until(50, || service.calls.load(Ordering::SeqCst) > 0).await,
        "cleanup should have run before shutdown"
    );

    runner.shutdown().await;
    let after_shutdown = service.calls.load(Ordering::SeqCst);

    // No further calls once the loop has been cancelled and joined.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        service.calls.load(Ordering::SeqCst),
        after_shutdown,
        "no cleanup should run after shutdown"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logs_deleted_count_when_rows_removed() {
    // A non-zero row count exercises the "purged expired sessions" info branch.
    let service = Arc::new(MockAuthService::returning(3));
    let parent = tracing::Span::none();

    let runner = SessionCleanupRunner::start_with_interval(
        Arc::clone(&service) as Arc<dyn AuthService>,
        Duration::from_millis(10),
        &parent,
    );

    assert!(
        wait_until(50, || service.calls.load(Ordering::SeqCst) > 0).await,
        "cleanup should have run at least once"
    );

    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn continues_running_after_cleanup_error() {
    // A failing service exercises the error branch; the loop must keep ticking.
    let service = Arc::new(MockAuthService::failing());
    let parent = tracing::Span::none();

    let runner = SessionCleanupRunner::start_with_interval(
        Arc::clone(&service) as Arc<dyn AuthService>,
        Duration::from_millis(10),
        &parent,
    );

    // At least two calls proves the loop survived the first error.
    assert!(
        wait_until(50, || service.calls.load(Ordering::SeqCst) >= 2).await,
        "runner should keep ticking after an error"
    );

    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn production_start_skips_immediate_tick() {
    // Exercises the production `start()` wrapper (hourly interval). The first
    // tick is consumed, so no cleanup fires before we shut down.
    let service = Arc::new(MockAuthService::new());
    let parent = tracing::Span::none();

    let runner = SessionCleanupRunner::start(Arc::clone(&service) as Arc<dyn AuthService>, &parent);

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        service.calls.load(Ordering::SeqCst),
        0,
        "hourly runner must not fire immediately"
    );

    runner.shutdown().await;
}
