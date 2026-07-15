//! Tests for [`TargetSuppressor`] prefix matching and the ERROR carve-out.

use tracing::Level;

use crate::logging::suppress::TargetSuppressor;

#[test]
fn default_suppresses_nothing() {
    let s = TargetSuppressor::default();
    assert!(!s.is_suppressed("hickory_resolver::recursor::handle", &Level::WARN));
    assert!(!s.is_suppressed("wardnetd::dns", &Level::WARN));
}

#[test]
fn matches_target_subtree_by_prefix() {
    let s = TargetSuppressor::new(vec!["hickory_resolver::recursor".to_owned()]);
    assert!(s.is_suppressed("hickory_resolver::recursor", &Level::WARN));
    assert!(s.is_suppressed("hickory_resolver::recursor::handle", &Level::WARN));
}

#[test]
fn leaves_other_targets_alone() {
    let s = TargetSuppressor::new(vec!["hickory_resolver::recursor".to_owned()]);
    assert!(!s.is_suppressed("hickory_resolver::pool", &Level::WARN));
    assert!(!s.is_suppressed("wardnetd::dns", &Level::WARN));
}

#[test]
fn suppresses_levels_below_warn() {
    let s = TargetSuppressor::new(vec!["hickory_resolver::recursor".to_owned()]);
    assert!(s.is_suppressed("hickory_resolver::recursor", &Level::INFO));
    assert!(s.is_suppressed("hickory_resolver::recursor", &Level::DEBUG));
}

#[test]
fn never_suppresses_error() {
    // A noise filter must not become a blind spot: a dependency logging ERROR
    // is saying something genuinely broke, and the admin needs to see it in
    // both the live stream and the recent-errors buffer.
    let s = TargetSuppressor::new(vec!["hickory_resolver::recursor".to_owned()]);
    assert!(!s.is_suppressed("hickory_resolver::recursor::handle", &Level::ERROR));
}

#[test]
fn empty_prefix_is_discarded() {
    // An empty prefix matches every target — a config typo must not blank the
    // admin UI.
    let s = TargetSuppressor::new(vec![String::new()]);
    assert!(!s.is_suppressed("wardnetd::dns", &Level::WARN));
}
