//! Tests for [`SessionCleanupRunner`]. Drives the runner with a recording
//! mock [`AuthService`] and asserts it calls `cleanup_expired_sessions` on
//! its interval (under an admin context), skips the immediate first tick, and
//! stops on shutdown.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use wardnet_common::api::{WizardMode, WizardStep};

use crate::auth::SessionCleanupRunner;
use crate::auth::service::{LoginResult, WizardState};
use crate::auth::{CurrentUser, LoginAttempt};
use crate::error::AppError;
use crate::{AuthService, auth_context};
use uuid::Uuid;
use wardnet_common::auth::{AuthenticatedUser, UserRole};

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
    async fn issue_verified_session(
        &self,
        _user_id: uuid::Uuid,
        _remember_me: bool,
        _user_agent: Option<&str>,
    ) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn current_user(&self) -> Result<CurrentUser, AppError> {
        Ok(CurrentUser {
            user_id: Uuid::nil(),
            display_name: "admin".to_owned(),
            email: None,
            role: UserRole::Admin,
        })
    }
    async fn login(&self, _attempt: LoginAttempt<'_>) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn logout_session(&self, _token: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        unimplemented!()
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
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

/// Minimal `UserService` for the runner tests: records enrolment-cleanup calls
/// and can be told to fail, so the loop's independence from the session sweep
/// is observable.
struct MockUserService {
    enrolment_calls: AtomicU64,
    fail: bool,
}

impl MockUserService {
    fn new() -> Self {
        Self {
            enrolment_calls: AtomicU64::new(0),
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            enrolment_calls: AtomicU64::new(0),
            fail: true,
        }
    }

    fn enrolment_calls(&self) -> u64 {
        self.enrolment_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl crate::user::UserService for MockUserService {
    async fn cleanup_expired_enrolments(&self) -> Result<u64, AppError> {
        self.enrolment_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(AppError::Internal(anyhow::anyhow!("boom")));
        }
        Ok(0)
    }
    async fn list_users(&self) -> Result<Vec<crate::user::UserProfile>, AppError> {
        unimplemented!()
    }
    async fn get_user(&self, _id: Uuid) -> Result<crate::user::UserProfile, AppError> {
        unimplemented!()
    }
    async fn create_user(
        &self,
        _new_user: crate::user::NewUser,
    ) -> Result<crate::user::UserProfile, AppError> {
        unimplemented!()
    }
    async fn update_profile(
        &self,
        _id: Uuid,
        _display_name: &str,
        _email: Option<&str>,
    ) -> Result<crate::user::UserProfile, AppError> {
        unimplemented!()
    }
    async fn set_enabled(
        &self,
        _id: Uuid,
        _enabled: bool,
    ) -> Result<crate::user::UserProfile, AppError> {
        unimplemented!()
    }
    async fn set_role(
        &self,
        _id: Uuid,
        _role: UserRole,
    ) -> Result<crate::user::UserProfile, AppError> {
        unimplemented!()
    }
    async fn delete_user(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn list_credentials(
        &self,
        _id: Uuid,
    ) -> Result<Vec<wardnetd_data::repository::user_credential::CredentialSummary>, AppError> {
        unimplemented!()
    }
    async fn change_own_password(&self, _current: &str, _new: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn issue_enrolment(&self, _id: Uuid) -> Result<crate::user::EnrolmentInvite, AppError> {
        unimplemented!()
    }
    async fn list_enrolments(
        &self,
        _id: Uuid,
    ) -> Result<Vec<crate::user::EnrolmentSummary>, AppError> {
        unimplemented!()
    }
    async fn revoke_enrolment(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn redeem_enrolment(
        &self,
        _token: &str,
        _password: &str,
    ) -> Result<crate::user::UserProfile, AppError> {
        unimplemented!()
    }
    async fn available_methods(&self) -> Result<crate::user::service::AuthMethods, AppError> {
        unimplemented!()
    }
    async fn configure_oauth_provider(
        &self,
        _provider: crate::user::OauthProvider,
        _client_id: &str,
        _client_secret: Option<&str>,
        _enabled: bool,
    ) -> Result<crate::user::ProviderStatus, AppError> {
        unimplemented!()
    }
    async fn clear_oauth_provider(
        &self,
        _provider: crate::user::OauthProvider,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn start_oauth(
        &self,
        _provider: crate::user::OauthProvider,
        _return_to: crate::user::ReturnTo,
        _remember_me: bool,
    ) -> Result<crate::user::service::OauthRedirect, AppError> {
        unimplemented!()
    }
    async fn complete_oauth_callback(
        &self,
        _state: &str,
        _code: &str,
    ) -> Result<crate::user::OauthOutcome, AppError> {
        unimplemented!()
    }
    async fn unlink_oauth(
        &self,
        _user_id: Uuid,
        _provider: crate::user::OauthProvider,
    ) -> Result<u64, AppError> {
        unimplemented!()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runs_cleanup_on_tick_under_admin_context() {
    let service = Arc::new(MockAuthService::new());
    let parent = tracing::Span::none();

    let runner = SessionCleanupRunner::start_with_interval(
        Arc::clone(&service) as Arc<dyn AuthService>,
        Arc::new(MockUserService::new()),
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
        Arc::new(MockUserService::new()),
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
        Arc::new(MockUserService::new()),
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
        Arc::new(MockUserService::new()),
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
        Arc::new(MockUserService::new()),
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

    let runner = SessionCleanupRunner::start(
        Arc::clone(&service) as Arc<dyn AuthService>,
        Arc::new(MockUserService::new()),
        &parent,
    );

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        service.calls.load(Ordering::SeqCst),
        0,
        "hourly runner must not fire immediately"
    );

    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn also_purges_expired_enrolment_tokens() {
    // The runner owns both sweeps. `CONTEXT.md` used to state that enrolment
    // cleanup was NOT wired; this is the test that makes the corrected claim
    // true rather than aspirational.
    let service = Arc::new(MockAuthService::new());
    let users = Arc::new(MockUserService::new());
    let parent = tracing::Span::none();

    let runner = SessionCleanupRunner::start_with_interval(
        Arc::clone(&service) as Arc<dyn AuthService>,
        Arc::clone(&users) as Arc<dyn crate::user::UserService>,
        Duration::from_millis(10),
        &parent,
    );

    let fired = wait_until(50, || users.enrolment_calls() > 0).await;
    assert!(fired, "expired enrolment tokens should have been purged");

    runner.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_enrolment_sweep_does_not_stop_the_session_sweep() {
    // The two are handled independently on purpose: one subsystem erroring must
    // not silently stop the other from reclaiming storage.
    let service = Arc::new(MockAuthService::new());
    let users = Arc::new(MockUserService::failing());
    let parent = tracing::Span::none();

    let runner = SessionCleanupRunner::start_with_interval(
        Arc::clone(&service) as Arc<dyn AuthService>,
        Arc::clone(&users) as Arc<dyn crate::user::UserService>,
        Duration::from_millis(10),
        &parent,
    );

    // Both keep being attempted across ticks despite the enrolment failure.
    let sessions_ran = wait_until(50, || service.calls.load(Ordering::SeqCst) > 1).await;
    let enrolments_ran = wait_until(50, || users.enrolment_calls() > 1).await;
    assert!(sessions_ran, "the session sweep must keep running");
    assert!(
        enrolments_ran,
        "the enrolment sweep must keep being retried"
    );

    runner.shutdown().await;
}
