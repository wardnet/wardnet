//! Tests for the in-binary migration list.

use crate::migrations::{Severity, migrations};

#[test]
fn lists_polkit_power_rule_migration() {
    // Pins the first entry so an accidental edit to migration ids,
    // severities, or ordering surfaces at PR time. Migration ids
    // MUST NEVER change once shipped — operators have state files
    // referencing them.
    let list = migrations();
    assert!(
        !list.is_empty(),
        "migration list should contain at least 0001_polkit_power_rule"
    );
    let first = &list[0];
    assert_eq!(first.id, "0001_polkit_power_rule");
    assert_eq!(first.severity, Severity::Optional);
}

/// The unit refresh is deliberately listed twice under different ids. The
/// runner records each migration applied and skips it forever, so a box that
/// already ran `0003` would never pick up a later unit change — a follow-up id
/// re-running the same reconciler is how the fix reaches it. Pinned here so a
/// well-meaning "duplicate entry" cleanup has to read this comment first.
#[test]
fn unit_refresh_runs_again_under_a_follow_up_id() {
    let ids: Vec<&'static str> = migrations().iter().map(|m| m.id).collect();
    assert!(ids.contains(&"0003_wardnetd_unit_refresh"), "got {ids:?}");
    assert!(
        ids.contains(&"0004_wardnetd_unit_config_write"),
        "the ReadWritePaths=/etc/wardnet grant must reach existing installs; got {ids:?}"
    );
}

#[test]
fn migration_ids_are_unique() {
    let list = migrations();
    let mut ids: Vec<&'static str> = list.iter().map(|m| m.id).collect();
    ids.sort_unstable();
    let len_before = ids.len();
    ids.dedup();
    assert_eq!(
        ids.len(),
        len_before,
        "duplicate migration id detected in migrations()"
    );
}

#[test]
fn severity_variants_are_distinct() {
    assert_ne!(Severity::Required, Severity::Optional);
    // `Copy` + `PartialEq` derives covered by usage but exercised
    // here so accidental removal of either in a refactor surfaces
    // immediately at compile time.
    let a = Severity::Required;
    let b = a;
    assert_eq!(a, b);
}
