//! Whether the `wardnet` account survives the uninstall.
//!
//! Deleting the account frees its UID for reuse. If the retained database and
//! secret store are still owned by that UID at that moment, a later `useradd`
//! on the same host silently inherits the `WireGuard` private keys and the
//! backup passphrase — so this decision is the whole point of the step, and it
//! has to hold in the awkward cases as well as the happy one.

use crate::uninstall::{RetainedData, classify_retained_data, mentions_identifier};

#[test]
fn nothing_to_protect_when_the_data_directory_is_already_gone() {
    assert_eq!(
        classify_retained_data(false, false),
        RetainedData::Gone,
        "no data directory means the account is safe to remove"
    );
}

#[test]
fn kept_data_must_be_re_owned_before_the_user_goes() {
    assert_eq!(
        classify_retained_data(false, true),
        RetainedData::NeedsChown
    );
}

#[test]
fn a_successful_purge_leaves_nothing_to_protect() {
    assert_eq!(classify_retained_data(true, false), RetainedData::Gone);
}

#[test]
fn an_interface_is_matched_as_a_whole_name_not_a_substring() {
    // The typed teardown and the `ip link` sweep both report interfaces, and
    // the second defers to the first. A substring test would let a still-live
    // `wg_ward10` be swallowed by a report about `wg_ward1`, under-reporting an
    // interface that is still up.
    let failure = "tunnel interface wg_ward1: netlink refused the delete";

    assert!(mentions_identifier(failure, "wg_ward1"));
    assert!(
        !mentions_identifier(failure, "wg_ward10"),
        "wg_ward10 must not be considered already-reported by a wg_ward1 failure"
    );
}

#[test]
fn interface_matching_ignores_surrounding_punctuation() {
    assert!(mentions_identifier(
        "inbound server wg_wardin0: device busy",
        "wg_wardin0"
    ));
    assert!(mentions_identifier(
        "wireguard interface wg_ward0 could not be removed",
        "wg_ward0"
    ));
}

#[test]
fn a_purge_that_did_not_delete_the_tree_keeps_the_user() {
    // The dangerous case: `--purge` was requested, `remove_dir_all` failed, and
    // the secret store is still on disk. Removing the account here would free
    // its UID while it still owns those files.
    assert_eq!(
        classify_retained_data(true, true),
        RetainedData::PurgeIncomplete,
        "a failed purge must not free the UID that still owns the secret store"
    );
}
