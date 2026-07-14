//! Runner-scheduling tests: the retry ladder, the CA rate-limit sit-out, and
//! the out-of-band [`TlsRetryNudge`] — all on paused tokio time.
//!
//! [`TlsRetryNudge`]: super::super::runner::TlsRetryNudge

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};

use crate::error::AppError;

use super::super::{TlsService, TlsStatus, TlsStatusResponse};

// ── Renewal retry backoff ─────────────────────────────────────────────────────
//
// A failed issuance used to wait for the next 12h tick. The failure that
// prompted this is transient by nature: a network registered seconds ago has no
// published DNS record yet, so the cloud rejects the ACME challenge with
// "network is not yet active" — and the daemon then sat certificate-less for
// half a day over a condition that clears in seconds.

#[test]
fn first_retry_after_a_failure_is_soon_not_a_full_cycle() {
    let first = super::super::runner::next_retry(None);
    assert_eq!(first, std::time::Duration::from_secs(30));
    assert!(
        first < std::time::Duration::from_hours(12),
        "a failure must not wait for the next renewal cycle"
    );
}

#[test]
fn retry_backoff_doubles_and_is_capped() {
    let mut d = super::super::runner::next_retry(None);
    let mut seen = vec![d];
    for _ in 0..10 {
        d = super::super::runner::next_retry(Some(d));
        seen.push(d);
    }

    // Doubles from 30s...
    assert_eq!(seen[0], std::time::Duration::from_secs(30));
    assert_eq!(seen[1], std::time::Duration::from_mins(1));
    assert_eq!(seen[2], std::time::Duration::from_mins(2));

    // ...and never exceeds the ceiling, however long the outage lasts.
    let cap = std::time::Duration::from_mins(15);
    assert!(
        seen.iter().all(|d| *d <= cap),
        "backoff must stay capped at 15m, got {seen:?}"
    );
    assert_eq!(*seen.last().unwrap(), cap, "should settle at the ceiling");
}

// ── Rate-limit-aware backoff + the out-of-band nudge (issue #886 follow-up) ──
//
// Two gaps let a broken issuance flow burn half a day and the CA's goodwill:
// the register-time provisioning task's failure was invisible to the runner
// (next attempt = the 12h tick), and the exponential ladder kept hammering a
// CA that was already answering "too many failed authorizations".

#[test]
fn rate_limit_errors_are_classified_as_such() {
    use super::super::runner::{FailureClass, classify_failure};
    assert_eq!(
        classify_failure(
            "upstream unavailable: ACME issuance failed: API error: too many failed \
             authorizations (5) for \"my.wardnet.services\" in the last 1h0m0s \
             (urn:ietf:params:acme:error:rateLimited)"
        ),
        FailureClass::RateLimited
    );
    assert_eq!(
        classify_failure("urn:ietf:params:acme:error:rateLimited"),
        FailureClass::RateLimited
    );
    assert_eq!(
        classify_failure(
            "API error: Order's status (\"invalid\") is not acceptable for finalization"
        ),
        FailureClass::Transient
    );
}

/// Counting mock: `ensure_certificate` returns a scripted sequence.
struct ScriptedTls {
    calls: std::sync::atomic::AtomicUsize,
    script: Vec<Result<TlsStatus, String>>,
}

#[async_trait]
impl TlsService for ScriptedTls {
    async fn ensure_certificate(&self) -> Result<TlsStatus, AppError> {
        let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match self.script.get(n).cloned().unwrap_or_else(|| {
            Ok(TlsStatus::UpToDate {
                domain: "t.example".into(),
                not_after: Utc::now() + Duration::days(60),
            })
        }) {
            Ok(status) => Ok(status),
            Err(msg) => Err(AppError::UpstreamUnavailable(msg)),
        }
    }
    async fn status(&self) -> Result<TlsStatus, AppError> {
        Ok(TlsStatus::NotConfigured)
    }
    async fn mark_provisioning_started(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn provisioning_status(&self) -> Result<TlsStatusResponse, AppError> {
        unimplemented!("not exercised by runner tests")
    }
    async fn teardown(&self) -> Result<(), AppError> {
        Ok(())
    }
}

fn calls(tls: &Arc<ScriptedTls>) -> usize {
    tls.calls.load(std::sync::atomic::Ordering::SeqCst)
}

/// A healthy runner sits on the 12h cadence; a nudge (register-time issuance
/// failed elsewhere) pulls the next attempt to the backoff floor instead.
#[tokio::test(start_paused = true)]
async fn a_nudge_schedules_a_retry_at_the_backoff_floor() {
    let tls = Arc::new(ScriptedTls {
        calls: std::sync::atomic::AtomicUsize::new(0),
        script: vec![],
    });
    let nudge = super::super::runner::TlsRetryNudge::default();
    let runner = super::super::runner::TlsRenewalRunner::start(
        tls.clone(),
        crate::entitlement::Entitlement::shared(),
        nudge.clone(),
        &tracing::Span::none(),
    );

    // Startup tick runs immediately and succeeds → next attempt is 12h away.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert_eq!(calls(&tls), 1);

    nudge.nudge();
    // The nudge must NOT retry instantly (that replays the just-lost race)...
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    assert_eq!(calls(&tls), 1, "no instant replay after a nudge");
    // ...but well before the 12h tick — at the 30s backoff floor.
    tokio::time::sleep(std::time::Duration::from_secs(40)).await;
    assert_eq!(calls(&tls), 2, "nudge pulls the retry to the backoff floor");

    runner.shutdown().await;
}

/// A rate-limited failure must sit out the CA's window (1h), not keep
/// climbing the 30s→15m ladder that tripped the limit in the first place.
#[tokio::test(start_paused = true)]
async fn a_rate_limited_failure_backs_off_for_the_full_window() {
    let tls = Arc::new(ScriptedTls {
        calls: std::sync::atomic::AtomicUsize::new(0),
        script: vec![Err(
            "too many failed authorizations (urn:ietf:params:acme:error:rateLimited)".into(),
        )],
    });
    let runner = super::super::runner::TlsRenewalRunner::start(
        tls.clone(),
        crate::entitlement::Entitlement::shared(),
        super::super::runner::TlsRetryNudge::default(),
        &tracing::Span::none(),
    );

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert_eq!(calls(&tls), 1);

    // Under the transient ladder the retry would land within 15m. It must not.
    tokio::time::sleep(std::time::Duration::from_mins(20)).await;
    assert_eq!(
        calls(&tls),
        1,
        "rate-limited: no retry inside the CA window"
    );

    // After the full hour, we try again.
    tokio::time::sleep(std::time::Duration::from_mins(45)).await;
    assert_eq!(calls(&tls), 2, "retry resumes once the window has passed");

    runner.shutdown().await;
}

/// Review follow-up: a nudge must never SHORTEN an in-force backoff. The
/// transient ladder caps at 15m, so feeding a 1h rate-limit backoff through
/// `next_retry` would shrink it — re-attempting inside the CA's window with
/// the exact burst the long backoff exists to prevent.
#[tokio::test(start_paused = true)]
async fn a_nudge_does_not_shorten_a_rate_limit_backoff() {
    let tls = Arc::new(ScriptedTls {
        calls: std::sync::atomic::AtomicUsize::new(0),
        script: vec![Err(
            "too many failed authorizations (urn:ietf:params:acme:error:rateLimited)".into(),
        )],
    });
    let nudge = super::super::runner::TlsRetryNudge::default();
    let runner = super::super::runner::TlsRenewalRunner::start(
        tls.clone(),
        crate::entitlement::Entitlement::shared(),
        nudge.clone(),
        &tracing::Span::none(),
    );

    // Startup tick fails rate-limited → 1h backoff in force.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert_eq!(calls(&tls), 1);

    // A register-time failure nudges. The retry must still respect the hour.
    nudge.nudge();
    tokio::time::sleep(std::time::Duration::from_mins(20)).await;
    assert_eq!(
        calls(&tls),
        1,
        "nudge must not shorten the rate-limit window"
    );
    tokio::time::sleep(std::time::Duration::from_mins(45)).await;
    assert_eq!(calls(&tls), 2, "retry lands after the window as scheduled");

    runner.shutdown().await;
}

/// Review follow-up: only the CA's rate-limit answer is a rate limit — a
/// broad "too many" substring caught unrelated transients ("too many open
/// files", "too many redirects") and cost an hour where 30s was right.
#[test]
fn unrelated_too_many_errors_stay_transient() {
    use super::super::runner::{FailureClass, classify_failure};
    assert_eq!(
        classify_failure("error sending request: too many redirects"),
        FailureClass::Transient
    );
    assert_eq!(
        classify_failure("Too many open files (os error 24)"),
        FailureClass::Transient
    );
    assert_eq!(
        classify_failure("too many failed authorizations for \"x\" in the last 1h0m0s"),
        FailureClass::RateLimited
    );
}

/// Review follow-up: a nudge is "make sure a retry is scheduled soon", not
/// "push the retry out". Repeated nudges (an admin re-running the wizard while
/// DNS misbehaves) must not keep resetting the countdown.
#[tokio::test(start_paused = true)]
async fn repeated_nudges_do_not_push_the_retry_out() {
    let tls = Arc::new(ScriptedTls {
        calls: std::sync::atomic::AtomicUsize::new(0),
        script: vec![],
    });
    let nudge = super::super::runner::TlsRetryNudge::default();
    let runner = super::super::runner::TlsRenewalRunner::start(
        tls.clone(),
        crate::entitlement::Entitlement::shared(),
        nudge.clone(),
        &tracing::Span::none(),
    );

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert_eq!(calls(&tls), 1); // startup success → 12h cadence

    // First nudge schedules a retry 30s out; a second nudge 20s later must
    // NOT restart the countdown.
    nudge.nudge();
    tokio::time::sleep(std::time::Duration::from_secs(20)).await;
    nudge.nudge();
    tokio::time::sleep(std::time::Duration::from_secs(15)).await; // t = +35s
    assert_eq!(
        calls(&tls),
        2,
        "the retry must land ~30s after the FIRST nudge, not the last"
    );

    runner.shutdown().await;
}
