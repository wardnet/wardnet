//! What each uninstall tier claims it will touch.
//!
//! These matter because the inventory is also exactly what `--dry-run` prints:
//! a dry run that under-reports is worse than no dry run at all.

use crate::uninstall::inventory::{
    Action, DATA_DIR, Item, Kind, RETAINED_DATA, UNITS, build_inventory, render_plan,
};

fn targets(items: &[Item]) -> Vec<String> {
    items.iter().map(|i| i.target.clone()).collect()
}

fn removed(items: &[Item]) -> Vec<String> {
    items
        .iter()
        .filter(|i| i.action == Action::Remove)
        .map(|i| i.target.clone())
        .collect()
}

fn kept(items: &[Item]) -> Vec<String> {
    items
        .iter()
        .filter(|i| i.action == Action::Keep)
        .map(|i| i.target.clone())
        .collect()
}

#[test]
fn default_tier_keeps_the_database_and_secret_store() {
    let items = build_inventory(false, false);

    assert_eq!(kept(&items), RETAINED_DATA.to_vec());
}

#[test]
fn retained_secrets_are_labelled_a_directory_not_a_file() {
    // `secrets/` is a directory and the database entries are files. The plan
    // prints the kind, so mislabelling misdescribes what will be touched.
    let items = build_inventory(false, false);
    for item in items.iter().filter(|i| i.action == Action::Keep) {
        let expected = if item.target.ends_with("/secrets") {
            Kind::Directory
        } else {
            Kind::File
        };
        assert_eq!(item.kind, expected, "wrong kind for {}", item.target);
    }
}

#[test]
fn default_tier_never_removes_the_data_directory_wholesale() {
    let items = build_inventory(false, false);

    assert!(
        !removed(&items).contains(&DATA_DIR.to_owned()),
        "the default tier must not remove {DATA_DIR}"
    );
}

#[test]
fn default_tier_still_removes_daemon_scratch_under_the_data_directory() {
    // Downloaded release artefacts and the staged post-upgrade binary are not
    // user data, so they go in both tiers.
    let removed = removed(&build_inventory(false, false));

    assert!(removed.contains(&"/var/lib/wardnet/updates".to_owned()));
    assert!(removed.contains(&"/var/lib/wardnet/postupgrade".to_owned()));
}

#[test]
fn purge_removes_the_data_directory_and_keeps_nothing() {
    let items = build_inventory(true, false);

    assert!(kept(&items).is_empty());
    assert!(removed(&items).contains(&DATA_DIR.to_owned()));
}

#[test]
fn every_tier_removes_the_units_the_installer_wrote() {
    for purge in [false, true] {
        let removed = removed(&build_inventory(purge, false));
        for unit in UNITS {
            assert!(
                removed.contains(&format!("/etc/systemd/system/{unit}")),
                "{unit} missing from inventory (purge={purge})"
            );
        }
    }
}

#[test]
fn never_touches_the_dev_only_unit() {
    // `wardnetd-dev.service` exists in deploy/ but install.sh never places it
    // on a real host, so uninstall must leave it alone.
    let all = targets(&build_inventory(true, false)).join("\n");

    assert!(!all.contains("wardnetd-dev.service"));
}

#[test]
fn bare_metal_removes_the_host_drop_ins() {
    let removed = removed(&build_inventory(false, false));

    // The issue's inventory missed the sysctl drop-in; install.sh does write it.
    assert!(removed.contains(&"/etc/sysctl.d/99-wardnet.conf".to_owned()));
    assert!(removed.contains(&"/etc/dhcpcd.conf.d/wardnet.conf".to_owned()));
    assert!(removed.contains(&"/etc/modules-load.d/wardnet-watchdog.conf".to_owned()));
}

#[test]
fn container_mode_omits_drop_ins_the_installer_never_wrote() {
    let all = targets(&build_inventory(false, true)).join("\n");

    assert!(!all.contains("/etc/sysctl.d/99-wardnet.conf"));
    assert!(!all.contains("/etc/dhcpcd.conf.d/wardnet.conf"));
    assert!(!all.contains("/etc/modules-load.d/wardnet-watchdog.conf"));
}

#[test]
fn always_reports_the_kernel_state_it_will_remove() {
    // This is the whole point of issue #864: a stopped daemon must not still
    // be filtering traffic, and a dry run must say so.
    let items = build_inventory(false, false);
    let runtime: Vec<&Item> = items.iter().filter(|i| i.kind == Kind::Runtime).collect();

    assert_eq!(runtime.len(), 2);
    assert!(runtime.iter().all(|i| i.action == Action::Remove));

    let plan = render_plan(&items);
    assert!(plan.contains("inet wardnet"));
    assert!(plan.contains("wg_ward*"));
}

#[test]
fn never_promises_a_conntrack_flush_it_does_not_perform() {
    // The uninstaller does not flush conntrack: a global `conntrack -F` would
    // take out every flow on the host (Docker's included), and a targeted
    // flush needs the device IPs, which means the database this command may
    // be deleting. Listing it in the plan anyway would make `--dry-run` lie.
    let plan = render_plan(&build_inventory(false, false));

    assert!(
        !plan.contains("conntrack"),
        "plan promises a conntrack flush"
    );
}

#[test]
fn removes_the_service_user_and_both_binaries() {
    let items = build_inventory(false, false);
    let removed = removed(&items);

    assert!(removed.contains(&"/usr/local/bin/wardnetd".to_owned()));
    assert!(removed.contains(&"/usr/local/bin/wardnetd.old".to_owned()));
    assert!(
        items
            .iter()
            .any(|i| i.kind == Kind::User && i.target == "wardnet")
    );
}

#[test]
fn removes_its_own_escape_hatch_script() {
    let removed = removed(&build_inventory(false, false));

    assert!(removed.contains(&"/usr/local/sbin/wardnet-uninstall".to_owned()));
}

#[test]
fn plan_distinguishes_kept_entries_from_removed_ones() {
    let plan = render_plan(&build_inventory(false, false));

    assert!(plan.contains("KEEP"));
    assert!(plan.contains("remove"));
    // The retained entries must carry the hint that --purge destroys them.
    assert!(plan.contains("--purge"));
}

#[test]
fn purge_plan_warns_that_data_is_destroyed() {
    let plan = render_plan(&build_inventory(true, false));

    assert!(plan.contains("DESTROYS"));
}
