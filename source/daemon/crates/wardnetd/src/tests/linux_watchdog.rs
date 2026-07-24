//! Tests for the `/dev/watchdog` open-failure classification (issue #214).

use std::io::{Error, ErrorKind};
use std::path::PathBuf;

use wardnetd_services::system::WatchdogOps;

use crate::system::linux_watchdog::{LinuxWatchdog, OpenFailure, classify_open_error};

#[test]
fn classify_maps_each_open_failure() {
    assert_eq!(
        classify_open_error(&Error::from(ErrorKind::NotFound)),
        OpenFailure::Absent
    );
    assert_eq!(
        classify_open_error(&Error::from(ErrorKind::ResourceBusy)),
        OpenFailure::BusyElsewhere
    );
    // Raw EBUSY (16) even if it isn't surfaced as ResourceBusy.
    assert_eq!(
        classify_open_error(&Error::from_raw_os_error(16)),
        OpenFailure::BusyElsewhere
    );
    assert_eq!(
        classify_open_error(&Error::from(ErrorKind::PermissionDenied)),
        OpenFailure::PermissionDenied
    );
    assert_eq!(
        classify_open_error(&Error::from(ErrorKind::Other)),
        OpenFailure::Other
    );
}

#[test]
fn open_absent_device_is_unavailable_not_a_panic() {
    // ENOENT path → the "no device present" arm → an unavailable, no-op
    // instance the daemon can still run with.
    let wd = LinuxWatchdog::open(PathBuf::from("/definitely/not/a/watchdog/xyz"), 15);
    assert!(!wd.is_available());
}

#[test]
fn disabled_instance_is_unavailable() {
    assert!(!LinuxWatchdog::disabled().is_available());
}
