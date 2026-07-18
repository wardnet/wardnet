//! Tests for the shared `WireGuard` interface removal helpers.
//!
//! Only the pure error classifier and the argument-validation path are
//! exercised here — the netlink round-trips are covered by the opt-in
//! `kernel_*` tests in `tunnel_interface_wireguard` and
//! `inbound_wg_interface_wireguard`.

use crate::wireguard_interface::{
    delete_wireguard_interface, is_interface_absent_error, remove_stale_link,
};

#[test]
fn absent_error_enodev_is_absent() {
    let err = std::io::Error::from_raw_os_error(libc::ENODEV);
    assert!(is_interface_absent_error(&err));
}

#[test]
fn absent_error_enoent_is_absent() {
    let err = std::io::Error::from_raw_os_error(libc::ENOENT);
    assert!(is_interface_absent_error(&err));
}

/// A `NotFound` error without a raw OS errno (e.g. synthesized by the
/// userspace backend) still counts as "interface absent".
#[test]
fn absent_error_not_found_kind_without_errno_is_absent() {
    let err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such socket");
    assert!(is_interface_absent_error(&err));
}

/// A stale userspace control socket with no `wireguard-go` listening surfaces
/// as `ECONNREFUSED` — the interface is effectively dead, so it counts as
/// absent (the old empty-`apply` path returned Ok here too).
#[test]
fn absent_error_econnrefused_is_absent() {
    let err = std::io::Error::from_raw_os_error(libc::ECONNREFUSED);
    assert!(is_interface_absent_error(&err));
}

/// A permission error is a real failure, not an absent interface — removal
/// must propagate it instead of pretending it was a no-op.
#[test]
fn absent_error_eperm_is_not_absent() {
    let err = std::io::Error::from_raw_os_error(libc::EPERM);
    assert!(!is_interface_absent_error(&err));
}

#[test]
fn absent_error_other_kind_is_not_absent() {
    let err = std::io::Error::other("netlink payload mismatch");
    assert!(!is_interface_absent_error(&err));
}

/// Deleting an interface that does not exist must never report a removal.
///
/// The exact outcome depends on the environment — an absent-classified error
/// (no such device, or no `WireGuard` genl family at all) yields `Ok(false)`,
/// while an unprivileged environment surfaces a permission error as `Err` —
/// but `Ok(true)` ("removed") is wrong everywhere.
#[tokio::test]
async fn delete_nonexistent_interface_is_not_a_removal() {
    if let Ok(removed) = delete_wireguard_interface("wgtest832x", "wireguard interface") {
        assert!(!removed, "a nonexistent interface cannot have been removed");
    }
}

/// `remove_stale_link` on a nonexistent interface is a silent no-op: the
/// `ip link show` probe fails (whether `ip` is missing or the link is), so
/// no delete is attempted.
#[tokio::test]
async fn remove_stale_link_is_noop_when_interface_absent() {
    remove_stale_link("wgtest832y", "wireguard interface").await;
}

/// An interface name the kernel would reject (too long) fails validation
/// before any netlink traffic.
#[test]
fn delete_rejects_invalid_interface_name() {
    let err = delete_wireguard_interface(
        "this-name-is-far-too-long-for-an-interface",
        "wireguard interface",
    )
    .expect_err("invalid interface name should be rejected");
    assert!(
        err.to_string().contains("invalid interface name"),
        "unexpected error: {err}"
    );
}
