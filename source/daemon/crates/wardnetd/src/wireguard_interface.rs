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

/// Delete a `WireGuard` interface by name, logging the outcome.
///
/// `Device::delete` is the crate's real removal path (netlink `DelLink` on
/// the Linux kernel backend). Applying an empty `DeviceUpdate` instead would
/// only layer a no-op config change on top of the existing interface — the
/// kernel backend never deletes on `apply`.
///
/// `label` names the interface kind in log and error messages (e.g.
/// "wireguard interface", "inbound wireguard server").
///
/// Returns `Ok(true)` when an interface was actually removed and `Ok(false)`
/// when it was already absent (idempotent no-op; logged at debug, not as a
/// removal). An absent-classified error from either the get or the delete
/// (the interface can vanish between the two, e.g. a concurrent teardown)
/// counts as already absent. Real failures (e.g. permission errors) are
/// returned as `Err`.
pub fn delete_wireguard_interface(interface_name: &str, label: &str) -> anyhow::Result<bool> {
    let iface: InterfaceName = interface_name
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid interface name: {e}"))?;

    match Device::get(&iface, Backend::default()).and_then(Device::delete) {
        Ok(()) => {
            tracing::info!(interface = %interface_name, "{label} {interface_name} removed");
            Ok(true)
        }
        Err(e) if is_interface_absent_error(&e) => {
            tracing::debug!(
                interface = %interface_name,
                "{label} {interface_name} already absent, nothing to remove"
            );
            Ok(false)
        }
        Err(e) => Err(anyhow::anyhow!(
            "failed to remove {label} {interface_name}: {e}"
        )),
    }
}

/// Best-effort removal of any same-named link via `ip link`, regardless of
/// link type.
///
/// Deliberately a shell-out rather than [`delete_wireguard_interface`]: the
/// `WireGuard` netlink family cannot see links of the wrong type or ones
/// left partially created by a crash, while `ip link delete` reaps them all.
/// Used by the create paths to clear stale interfaces before re-creating
/// (avoiding "Address already assigned" errors).
pub async fn remove_stale_link(interface_name: &str, label: &str) {
    let check = tokio::process::Command::new("ip")
        .args(["link", "show", interface_name])
        .output()
        .await;
    if check.is_ok_and(|o| o.status.success()) {
        tracing::info!(
            interface = %interface_name,
            "removing stale {label} {interface_name} before re-creation"
        );
        let _ = tokio::process::Command::new("ip")
            .args(["link", "delete", interface_name])
            .output()
            .await;
    }
}
