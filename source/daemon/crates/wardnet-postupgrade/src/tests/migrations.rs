//! Tests for the in-binary migration list.

use crate::migrations::{Severity, migrations};

#[test]
fn empty_list_until_first_migration_lands() {
    // The framework ships with no migrations — #215 (polkit power
    // rule) is the first consumer. This test pins the empty state
    // so an accidental insert here gets caught at PR time and the
    // author goes through the proper migration-add review.
    assert!(
        migrations().is_empty(),
        "migration list must stay empty until a deliberate migration is added"
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
