//! Unit tests for the `systemctl show` output parsing and the unit
//! allowlist.
//!
//! The handlers themselves shell out to systemd, so they are covered by
//! `auto-update-swap.spec.ts` in the e2e suite rather than here. What is
//! worth testing in isolation is the parsing, because a silently-wrong
//! `show_property` would make the trust-anchor spec's assertions pass
//! vacuously.

use super::super::*;

#[test]
fn show_property_extracts_a_present_key() {
    let out = "ActiveState=failed\nResult=exit-code\nExecMainStatus=1\n";
    assert_eq!(show_property(out, "ActiveState"), "failed");
    assert_eq!(show_property(out, "Result"), "exit-code");
    assert_eq!(show_property(out, "ExecMainStatus"), "1");
}

#[test]
fn show_property_is_empty_for_a_missing_key() {
    assert_eq!(show_property("ActiveState=active\n", "Result"), "");
}

#[test]
fn show_property_does_not_match_a_key_that_is_only_a_suffix() {
    // `ExecMainStatus` must not be satisfied by `MainStatus=`; the prefix
    // match is anchored to the start of the line.
    assert_eq!(show_property("MainStatus=3\n", "ExecMainStatus"), "");
}

#[test]
fn allowlist_covers_only_the_two_units_the_suite_drives() {
    assert!(ALLOWED_UNITS.contains(&"wardnetd.service"));
    assert!(ALLOWED_UNITS.contains(&"wardnet-postupgrade.service"));
    assert!(!ALLOWED_UNITS.contains(&"wardnet-test-agent.service"));
}
