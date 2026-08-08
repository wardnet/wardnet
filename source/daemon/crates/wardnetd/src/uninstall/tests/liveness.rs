//! Interpreting `systemctl is-active` after the stop.
//!
//! Getting this wrong in the permissive direction is what deletes the binary
//! out from under a live daemon, forcing a SIGKILL that leaves the hardware
//! watchdog armed and reboots the host mid-uninstall. Only a confirmed stop
//! may proceed.

use crate::uninstall::ops::SystemctlError;
use crate::uninstall::{Liveness, classify_liveness};

fn status(code: Option<i32>, stderr: &str) -> Result<(), SystemctlError> {
    Err(SystemctlError::Status {
        args: "is-active --quiet wardnetd.service".to_owned(),
        code,
        stderr: stderr.to_owned(),
    })
}

#[test]
fn exit_zero_means_the_unit_is_still_running() {
    assert_eq!(classify_liveness(&Ok(())), Liveness::NotConfirmed);
}

#[test]
fn inactive_and_missing_units_are_confirmed_stopped() {
    assert_eq!(classify_liveness(&status(Some(3), "")), Liveness::Stopped);
    assert_eq!(classify_liveness(&status(Some(4), "")), Liveness::Stopped);
}

#[test]
fn a_host_without_systemd_counts_as_stopped() {
    // Container or chroot: nothing is supervising the daemon, so there is no
    // stop to confirm.
    assert_eq!(
        classify_liveness(&status(
            Some(1),
            "System has not been booted with systemd as init system"
        )),
        Liveness::Stopped
    );
    assert_eq!(
        classify_liveness(&status(Some(1), "Failed to connect to bus: No such file")),
        Liveness::Stopped
    );
    assert_eq!(
        classify_liveness(&Err(SystemctlError::Spawn(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no systemctl"
        )))),
        Liveness::Stopped
    );
}

#[test]
fn an_unexplained_failure_is_never_read_as_stopped() {
    // The dangerous default. A probe that failed for an unrelated reason tells
    // us nothing, and assuming "down" is the assumption that reboots the box.
    assert_eq!(
        classify_liveness(&status(Some(1), "Connection timed out")),
        Liveness::NotConfirmed
    );
    assert_eq!(
        classify_liveness(&status(None, "killed")),
        Liveness::NotConfirmed
    );
    assert_eq!(
        classify_liveness(&Err(SystemctlError::Spawn(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied"
        )))),
        Liveness::NotConfirmed
    );
}
