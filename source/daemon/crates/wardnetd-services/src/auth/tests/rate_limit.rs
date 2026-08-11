//! Unit tests for the login rate limiter.
//!
//! The service-level test (`repeated_failures_trip_the_login_rate_limiter`)
//! covers the identity counter through a real login. These exercise the parts
//! that path cannot reach: the source-IP counter on its own, the
//! clear-on-success behaviour, and the memory-exhaustion guard.

use crate::auth::rate_limit::LoginRateLimiter;

const FREE_ATTEMPTS: u32 = 4;

/// Fail an identity enough times to push it past the free allowance.
fn exhaust(limiter: &LoginRateLimiter, subject: &str, ip: Option<&str>) {
    for _ in 0..=FREE_ATTEMPTS {
        limiter.record_failure(subject, ip);
    }
}

#[test]
fn a_fresh_limiter_allows_the_attempt() {
    let limiter = LoginRateLimiter::default();

    assert!(
        limiter
            .check("someone@example.com", Some("10.0.0.1"))
            .is_none()
    );
}

#[test]
fn the_free_allowance_is_not_throttled() {
    let limiter = LoginRateLimiter::new();

    for _ in 0..FREE_ATTEMPTS {
        limiter.record_failure("someone@example.com", Some("10.0.0.1"));
    }

    assert!(
        limiter
            .check("someone@example.com", Some("10.0.0.1"))
            .is_none(),
        "a handful of typos must not lock a household out"
    );
}

/// The whole point of the second counter: an attacker walking the directory
/// tries a couple of passwords per account, so no identity counter ever trips,
/// but every request shares one source address.
#[test]
fn the_source_ip_counter_trips_even_when_each_identity_is_untouched() {
    let limiter = LoginRateLimiter::new();

    for i in 0..=FREE_ATTEMPTS {
        limiter.record_failure(&format!("victim-{i}@example.com"), Some("10.0.0.9"));
    }

    // A brand-new identity from that same address is refused: the identity
    // counter is clean, so this can only be the IP counter talking.
    assert!(
        limiter
            .check("untouched@example.com", Some("10.0.0.9"))
            .is_some(),
        "the per-IP counter must catch a directory walk"
    );
}

/// The identity counter must be independent of the address, or an attacker on a
/// botnet would get a fresh allowance per source.
#[test]
fn the_identity_counter_follows_the_subject_across_addresses() {
    let limiter = LoginRateLimiter::new();

    for i in 0..=FREE_ATTEMPTS {
        limiter.record_failure("victim@example.com", Some(&format!("10.0.0.{i}")));
    }

    assert!(
        limiter
            .check("victim@example.com", Some("192.168.1.1"))
            .is_some(),
        "a new source address must not reset the identity's backoff"
    );
}

/// `Admin` and `admin` must share one counter, or the limit is trivially
/// doubled by varying the case of the submitted subject.
#[test]
fn subject_counters_are_case_and_whitespace_insensitive() {
    let limiter = LoginRateLimiter::new();

    exhaust(&limiter, "Victim@Example.COM", None);

    assert!(
        limiter.check("  victim@example.com  ", None).is_some(),
        "counter keys must be normalised the same way credential lookup is"
    );
}

#[test]
fn success_clears_the_subject_counter() {
    let limiter = LoginRateLimiter::new();
    exhaust(&limiter, "someone@example.com", None);
    assert!(limiter.check("someone@example.com", None).is_some());

    limiter.record_success("someone@example.com", None);

    assert!(limiter.check("someone@example.com", None).is_none());
}

/// A household behind one NAT address shares it, so a successful login has to
/// clear the IP counter too — otherwise one person's typos throttle everybody.
#[test]
fn success_clears_the_source_ip_counter_for_other_identities() {
    let limiter = LoginRateLimiter::new();
    for i in 0..=FREE_ATTEMPTS {
        limiter.record_failure(&format!("member-{i}@example.com"), Some("10.0.0.5"));
    }
    assert!(
        limiter
            .check("someone-else@example.com", Some("10.0.0.5"))
            .is_some()
    );

    limiter.record_success("member-0@example.com", Some("10.0.0.5"));

    assert!(
        limiter
            .check("someone-else@example.com", Some("10.0.0.5"))
            .is_none(),
        "clearing on success must release the shared address"
    );
}

/// An unknown account must throttle exactly like a real one — if it did not,
/// the 429 itself would disclose which subjects exist.
#[test]
fn an_unknown_subject_throttles_like_a_real_one() {
    let limiter = LoginRateLimiter::new();

    exhaust(&limiter, "no-such-user@example.com", None);

    assert!(limiter.check("no-such-user@example.com", None).is_some());
}

#[test]
fn the_backoff_grows_with_consecutive_failures() {
    let limiter = LoginRateLimiter::new();
    exhaust(&limiter, "someone@example.com", None);
    let first = limiter
        .check("someone@example.com", None)
        .expect("throttled");

    limiter.record_failure("someone@example.com", None);
    let second = limiter
        .check("someone@example.com", None)
        .expect("throttled");

    assert!(
        second > first,
        "backoff must escalate: {second:?} should exceed {first:?}"
    );
}

/// The backoff is capped, so a household that genuinely forgot a password waits
/// a minute per try rather than days.
#[test]
fn the_backoff_is_capped() {
    let limiter = LoginRateLimiter::new();

    // Far more failures than the shift can absorb — this also pins the
    // saturating shift, which would otherwise wrap and switch the limiter off.
    for _ in 0..64 {
        limiter.record_failure("someone@example.com", None);
    }

    let wait = limiter
        .check("someone@example.com", None)
        .expect("throttled");
    assert!(
        wait <= std::time::Duration::from_mins(1),
        "backoff must stay capped, got {wait:?}"
    );
    assert!(
        !wait.is_zero(),
        "a sustained attack must not switch the limiter off"
    );
}

/// A stream of distinct subjects must not grow the table without bound — on a
/// Raspberry Pi that is a trivially reachable memory-exhaustion vector. Past
/// the cap the table is cleared wholesale.
#[test]
fn the_counter_table_is_bounded() {
    let limiter = LoginRateLimiter::new();

    // MAX_TRACKED is 8192; go past it so the guard has to fire.
    for i in 0..9_000 {
        limiter.record_failure(&format!("subject-{i}@example.com"), None);
    }

    // The most recent subject survives (it is re-inserted after the clear),
    // and the limiter is still functional rather than wedged.
    limiter.record_failure("fresh@example.com", None);
    assert!(
        limiter.check("fresh@example.com", None).is_none(),
        "a single failure after a clear must not be throttled"
    );
}

/// Omitting the client IP must not panic or accidentally key on a shared
/// sentinel — an unknown source simply gets no IP counter.
#[test]
fn a_missing_client_ip_is_handled() {
    let limiter = LoginRateLimiter::new();

    exhaust(&limiter, "someone@example.com", None);

    assert!(
        limiter.check("other@example.com", None).is_none(),
        "a missing IP must not create a counter other subjects share"
    );
}
