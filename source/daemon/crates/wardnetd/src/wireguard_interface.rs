//! Shared removal helpers for the `wireguard-control`-backed interface
//! implementations
//! ([`WireGuardTunnelInterface`](crate::tunnel_interface_wireguard::WireGuardTunnelInterface)
//! and
//! [`WireGuardInboundInterface`](crate::inbound_wg_interface_wireguard::WireGuardInboundInterface)),
//! so both delete interfaces through one mechanism with one error
//! classification.

use wireguard_control::{Backend, Device, InterfaceName};

/// Whether an error from [`Device::get`] or [`Device::delete`] means the
/// interface does not exist, as opposed to a real failure (e.g. a permission
/// error).
///
/// The kernel backend surfaces a missing interface as `ENODEV` ("No such
/// device"); the userspace backend reports a missing control socket as
/// `ENOENT`/`NotFound`, and a stale socket file left behind by a crashed
/// `wireguard-go` as `ConnectionRefused`. All mean there is nothing live to
/// remove, which [`delete_wireguard_interface`] treats as idempotent success.
#[must_use]
pub fn is_interface_absent_error(err: &std::io::Error) -> bool {
    matches!(err.raw_os_error(), Some(libc::ENODEV | libc::ENOENT))
        || matches!(
            err.kind(),
            std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
        )
}

/// Delete a `WireGuard` interface by name.
///
/// `Device::delete` is the crate's real removal path (netlink `DelLink` on
/// the Linux kernel backend). Applying an empty `DeviceUpdate` instead would
/// only layer a no-op config change on top of the existing interface — the
/// kernel backend never deletes on `apply`.
///
/// Returns `Ok(true)` when an interface was actually removed and `Ok(false)`
/// when it was already absent (idempotent no-op). An absent-classified error
/// from either the get or the delete (the interface can vanish between the
/// two, e.g. a concurrent teardown) counts as already absent. Real failures
/// (e.g. permission errors) are returned as `Err`.
pub fn delete_wireguard_interface(interface_name: &str) -> anyhow::Result<bool> {
    let iface: InterfaceName = interface_name
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid interface name: {e}"))?;

    match Device::get(&iface, Backend::default()).and_then(Device::delete) {
        Ok(()) => Ok(true),
        Err(e) if is_interface_absent_error(&e) => Ok(false),
        Err(e) => Err(anyhow::anyhow!(
            "failed to remove wireguard interface {interface_name}: {e}"
        )),
    }
}
