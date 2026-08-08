//! End-to-end behaviour of `wardnetd uninstall`, driven against a fake host.
//!
//! These cover the decisions that make the command safe: refusing without
//! privileges or confirmation, refusing to touch anything while the daemon may
//! still be alive, keeping the `wardnet` account whenever its data could still
//! be owned by that UID, and never reporting success over a recorded failure.

use crate::uninstall::tests::double::{FakeHost, inactive, indeterminate, not_booted, not_loaded};
use crate::uninstall::{UninstallArgs, run_with};

fn default_args() -> UninstallArgs {
    UninstallArgs {
        yes: true,
        ..UninstallArgs::default()
    }
}

/// A host where the daemon is stopped and nothing is left on disk.
fn clean_host() -> FakeHost {
    FakeHost::default()
}

#[tokio::test]
async fn dry_run_changes_nothing_and_needs_no_privileges() {
    let host = FakeHost {
        is_root: false,
        ..FakeHost::default()
    };
    let args = UninstallArgs {
        dry_run: true,
        ..UninstallArgs::default()
    };

    run_with(&args, &host)
        .await
        .expect("dry run should succeed");

    assert!(
        host.calls().is_empty(),
        "dry run touched the host: {:?}",
        host.calls()
    );
}

#[tokio::test]
async fn refuses_without_root() {
    let host = FakeHost {
        is_root: false,
        ..FakeHost::default()
    };

    let err = run_with(&default_args(), &host)
        .await
        .expect_err("should refuse");

    assert!(err.to_string().contains("must be run as root"), "got {err}");
    assert!(host.calls().is_empty(), "{:?}", host.calls());
}

#[tokio::test]
async fn refuses_when_there_is_no_terminal_and_no_yes() {
    let host = FakeHost {
        prompt_available: false,
        ..FakeHost::default()
    };
    let args = UninstallArgs::default();

    let err = run_with(&args, &host).await.expect_err("should refuse");

    assert!(err.to_string().contains("no terminal"), "got {err}");
}

#[tokio::test]
async fn a_wrong_confirmation_word_aborts_without_touching_anything() {
    let host = FakeHost {
        prompt_answer: "y".to_owned(), // not the required "yes"
        ..FakeHost::default()
    };

    run_with(&UninstallArgs::default(), &host)
        .await
        .expect("abort is not an error");

    assert_eq!(host.calls(), vec!["prompt"]);
}

#[tokio::test]
async fn purge_demands_the_word_in_full() {
    let host = FakeHost {
        prompt_answer: "yes".to_owned(), // not "PURGE"
        ..FakeHost::default()
    };
    let args = UninstallArgs {
        purge: true,
        ..UninstallArgs::default()
    };

    run_with(&args, &host).await.expect("abort is not an error");

    assert_eq!(host.calls(), vec!["prompt"]);
}

#[tokio::test]
async fn aborts_before_removing_anything_when_the_daemon_is_still_running() {
    // `is-active` exit 0 == still up.
    let host = FakeHost {
        is_active_result: None,
        ..FakeHost::default()
    };

    let err = run_with(&default_args(), &host)
        .await
        .expect_err("should refuse to proceed");

    assert!(err.to_string().contains("could not confirm"), "got {err}");
    assert!(
        !host.calls().iter().any(|c| c.starts_with("remove_path")),
        "removed something despite a live daemon: {:?}",
        host.calls()
    );
}

#[tokio::test]
async fn aborts_when_liveness_cannot_be_determined() {
    // A probe that failed for an unrelated reason says nothing about whether
    // the daemon is alive, and guessing "down" is the guess that reboots the
    // box via the still-armed watchdog.
    let host = FakeHost {
        is_active_result: Some(indeterminate()),
        ..FakeHost::default()
    };

    let err = run_with(&default_args(), &host)
        .await
        .expect_err("should refuse");

    assert!(err.to_string().contains("could not confirm"), "got {err}");
    assert!(!host.calls().iter().any(|c| c.starts_with("remove_path")));
}

#[tokio::test]
async fn proceeds_when_there_is_no_systemd() {
    let host = FakeHost {
        stop_result: Some(not_booted()),
        is_active_result: Some(not_booted()),
        ..FakeHost::default()
    };

    run_with(&default_args(), &host)
        .await
        .expect("should proceed");

    assert!(host.calls().iter().any(|c| c.starts_with("remove_path")));
}

#[tokio::test]
async fn tolerates_a_unit_that_is_not_loaded() {
    // The normal answer on a re-run or a partial install.
    let host = FakeHost {
        stop_result: Some(not_loaded()),
        is_active_result: Some(inactive()),
        ..FakeHost::default()
    };

    run_with(&default_args(), &host)
        .await
        .expect("an absent unit must not fail the uninstall");
}

#[tokio::test]
async fn a_clean_run_removes_the_units_and_the_user() {
    let host = clean_host();

    run_with(&default_args(), &host)
        .await
        .expect("should succeed");

    let calls = host.calls();
    for unit in crate::uninstall::inventory::UNITS {
        assert!(calls.contains(&format!("disable:{unit}")), "{calls:?}");
    }
    assert!(
        calls.contains(&"delete_user:wardnet".to_owned()),
        "{calls:?}"
    );
    assert!(calls.contains(&"daemon_reload".to_owned()), "{calls:?}");
}

#[tokio::test]
async fn retained_data_is_re_owned_before_the_user_is_removed() {
    let host = FakeHost {
        present: vec!["/var/lib/wardnet".to_owned()],
        reachable: vec!["/var/lib/wardnet".to_owned()],
        ..FakeHost::default()
    };

    run_with(&default_args(), &host)
        .await
        .expect("should succeed");

    let calls = host.calls();
    let chown = calls
        .iter()
        .position(|c| c.starts_with("chown:"))
        .expect("chowned");
    let userdel = calls
        .iter()
        .position(|c| c.starts_with("delete_user:"))
        .expect("removed user");
    assert!(chown < userdel, "user removed before the re-own: {calls:?}");
}

#[tokio::test]
async fn a_failed_re_own_keeps_the_user_and_fails_the_run() {
    let host = FakeHost {
        present: vec!["/var/lib/wardnet".to_owned()],
        reachable: vec!["/var/lib/wardnet".to_owned()],
        chown_fails: true,
        ..FakeHost::default()
    };

    let err = run_with(&default_args(), &host)
        .await
        .expect_err("should fail");

    assert!(err.to_string().contains("step(s) failed"), "got {err}");
    assert!(
        !host.calls().iter().any(|c| c.starts_with("delete_user")),
        "freed the UID that still owns the data: {:?}",
        host.calls()
    );
}

#[tokio::test]
async fn an_unreachable_data_directory_keeps_the_user() {
    // Present as a symlink but its target is unmounted: `chown -R -h` would
    // re-own the link, exit 0, and traverse nothing.
    let host = FakeHost {
        present: vec!["/var/lib/wardnet".to_owned()],
        reachable: Vec::new(),
        ..FakeHost::default()
    };

    let err = run_with(&default_args(), &host)
        .await
        .expect_err("should fail");

    assert!(err.to_string().contains("step(s) failed"), "got {err}");
    let calls = host.calls();
    assert!(!calls.iter().any(|c| c.starts_with("chown:")), "{calls:?}");
    assert!(
        !calls.iter().any(|c| c.starts_with("delete_user")),
        "{calls:?}"
    );
}

#[tokio::test]
async fn a_purge_that_left_the_tree_behind_keeps_the_user() {
    let host = FakeHost {
        present: vec!["/var/lib/wardnet".to_owned()],
        reachable: vec!["/var/lib/wardnet".to_owned()],
        ..FakeHost::default()
    };
    let args = UninstallArgs {
        purge: true,
        yes: true,
        ..UninstallArgs::default()
    };

    let err = run_with(&args, &host).await.expect_err("should fail");

    assert!(err.to_string().contains("step(s) failed"), "got {err}");
    assert!(
        !host.calls().iter().any(|c| c.starts_with("delete_user")),
        "freed the UID while the secret store survived: {:?}",
        host.calls()
    );
}

#[tokio::test]
async fn purge_deletes_the_resolved_tree_and_then_the_symlink() {
    let host = FakeHost {
        canonical: Some("/mnt/data/wardnet".into()),
        ..FakeHost::default()
    };
    let args = UninstallArgs {
        purge: true,
        yes: true,
        ..UninstallArgs::default()
    };

    run_with(&args, &host).await.expect("should succeed");

    let calls = host.calls();
    assert!(
        calls.contains(&"remove_path:/mnt/data/wardnet".to_owned()),
        "did not delete the real tree: {calls:?}"
    );
    assert!(
        calls.contains(&"remove_path:/var/lib/wardnet".to_owned()),
        "did not drop the symlink: {calls:?}"
    );
}

#[tokio::test]
async fn a_firewall_teardown_failure_fails_the_run() {
    // The worst possible outcome is reporting success over a live table.
    let host = FakeHost {
        teardown_failures: vec!["nftables table `inet wardnet`: permission denied".to_owned()],
        ..FakeHost::default()
    };

    let err = run_with(&default_args(), &host)
        .await
        .expect_err("should fail");

    assert!(err.to_string().contains("step(s) failed"), "got {err}");
}

#[tokio::test]
async fn an_interface_that_survives_the_sweep_is_reported() {
    let host = FakeHost {
        links_before: vec!["wg_ward0".to_owned()],
        links_after: vec!["wg_ward0".to_owned()],
        ..FakeHost::default()
    };

    let err = run_with(&default_args(), &host)
        .await
        .expect_err("should fail");

    assert!(
        err.to_string().contains("1 uninstall step(s) failed"),
        "got {err}"
    );
    assert!(host.calls().contains(&"delete_link:wg_ward0".to_owned()));
}

#[tokio::test]
async fn an_interface_removed_by_the_sweep_is_not_reported() {
    let host = FakeHost {
        links_before: vec!["wg_ward0".to_owned()],
        links_after: Vec::new(),
        ..FakeHost::default()
    };

    run_with(&default_args(), &host)
        .await
        .expect("a successfully swept interface is not a failure");
}

#[tokio::test]
async fn an_interface_is_not_counted_twice() {
    // Reported by the typed teardown and still present afterwards; the summary
    // must name it once, not twice.
    let host = FakeHost {
        teardown_failures: vec!["tunnel interface wg_ward0: device busy".to_owned()],
        links_before: vec!["wg_ward0".to_owned()],
        links_after: vec!["wg_ward0".to_owned()],
        ..FakeHost::default()
    };

    let err = run_with(&default_args(), &host)
        .await
        .expect_err("should fail");

    assert!(
        err.to_string().contains("1 uninstall step(s) failed"),
        "double-counted: {err}"
    );
}

#[tokio::test]
async fn a_failed_link_enumeration_is_reported_rather_than_ignored() {
    let host = FakeHost {
        links_error: true,
        ..FakeHost::default()
    };

    let err = run_with(&default_args(), &host)
        .await
        .expect_err("should fail");

    assert!(err.to_string().contains("step(s) failed"), "got {err}");
}

#[tokio::test]
async fn a_file_that_will_not_delete_fails_the_run() {
    let host = FakeHost {
        remove_path_fails: vec!["/usr/local/bin/wardnetd".to_owned()],
        ..FakeHost::default()
    };

    let err = run_with(&default_args(), &host)
        .await
        .expect_err("should fail");

    assert!(err.to_string().contains("step(s) failed"), "got {err}");
}

#[tokio::test]
async fn container_mode_never_touches_the_host_drop_ins() {
    let host = FakeHost::default();
    let args = UninstallArgs {
        yes: true,
        container_mode: true,
        ..UninstallArgs::default()
    };

    run_with(&args, &host).await.expect("should succeed");

    let calls = host.calls().join("\n");
    assert!(!calls.contains("/etc/sysctl.d/99-wardnet.conf"), "{calls}");
    assert!(
        !calls.contains("/etc/dhcpcd.conf.d/wardnet.conf"),
        "{calls}"
    );
    assert!(
        !calls.contains("/etc/modules-load.d/wardnet-watchdog.conf"),
        "{calls}"
    );
}

#[tokio::test]
async fn bare_metal_removes_the_host_drop_ins() {
    let host = clean_host();

    run_with(&default_args(), &host)
        .await
        .expect("should succeed");

    let calls = host.calls().join("\n");
    assert!(calls.contains("/etc/sysctl.d/99-wardnet.conf"), "{calls}");
    assert!(calls.contains("/etc/dhcpcd.conf.d/wardnet.conf"), "{calls}");
    assert!(
        calls.contains("/etc/modules-load.d/wardnet-watchdog.conf"),
        "{calls}"
    );
}

#[tokio::test]
async fn is_idempotent_on_an_already_clean_host() {
    let host = clean_host();

    run_with(&default_args(), &host).await.expect("first run");
    run_with(&default_args(), &host).await.expect("second run");
}
