use chrono::{TimeDelta, Utc};

use super::within_renewal_window;

#[test]
fn outside_window_is_not_due() {
    let now = Utc::now();
    let not_after = now + TimeDelta::days(60);
    assert!(!within_renewal_window(not_after, now));
}

#[test]
fn inside_window_is_due() {
    let now = Utc::now();
    let not_after = now + TimeDelta::days(20);
    assert!(within_renewal_window(not_after, now));
}

#[test]
fn already_expired_is_due() {
    let now = Utc::now();
    let not_after = now - TimeDelta::days(1);
    assert!(within_renewal_window(not_after, now));
}

#[test]
fn exactly_at_window_boundary_is_due() {
    let now = Utc::now();
    // Just inside 30 days ⇒ due.
    let not_after = now + TimeDelta::days(30) - TimeDelta::seconds(1);
    assert!(within_renewal_window(not_after, now));
}
