//! Shutdown-cause classification and teardown of the kernel state the daemon
//! created.
//!
//! # Runtime state
//!
//! "Runtime state" is the state the daemon installs in the *kernel*, as
//! distinct from the files the installer put on disk: the `inet wardnet`
//! nftables table and the `wg_ward*` `WireGuard` interfaces. Historically none
//! of it was ever removed — `systemctl stop wardnetd` left every
//! forward/input/NAT rule live until the next reboot, so a stopped daemon was
//! still filtering the user's traffic (issue #864).
//!
//! # Why teardown is gated on the cause
//!
//! The daemon restarts itself far more often than it is stopped: the
//! auto-update runner, the rollback path, and the admin "Restart" button all
//! cancel the shutdown token to hand over to a fresh process under
//! systemd's `Restart=always`. Tearing runtime state down on *those* shutdowns
//! would be actively harmful, because tunnels have **no synchronous boot
//! reconcile** — `wg_ward*` interfaces are only rebuilt lazily by the tunnel
//! monitor's health-check tick, so every six-hourly auto-update would drop
//! the user's VPN tunnels for a full health-check interval.
//!
//! So teardown runs only for [`ShutdownCause::Signal`]. The nftables side
//! would in fact be safe either way (startup reconcile flushes the table
//! anyway), but keeping both halves on the same gate means one rule to reason
//! about rather than two.
//!
//! A human typing `systemctl restart wardnetd` is indistinguishable from
//! `systemctl stop` — both arrive as a bare SIGTERM — so it classifies as
//! `Signal` and tears down. That costs one bring-up cycle but nothing more:
//! startup reconcile rebuilds the nftables table, and because teardown records
//! each tunnel as `Down` (see [`TunnelTeardown`]), the same reconcile's
//! per-device `apply_rule` pass brings the tunnels back up on demand.

use std::sync::Arc;

use wardnet_common::tunnel::TunnelStatus;
use wardnetd_services::inbound_wg::InboundWgInterface;
use wardnetd_services::inbound_wg::service::INBOUND_WG_INTERFACE;
use wardnetd_services::routing::FirewallManager;
use wardnetd_services::tunnel::interface::TUNNEL_INTERFACE_PREFIX;
use wardnetd_services::tunnel::{TunnelInterface, TunnelService};

#[cfg(test)]
mod tests;

/// What caused the daemon to begin shutting down.
///
/// Produced by the `shutdown_signal` future in `main.rs`, which selects over
/// SIGINT, SIGTERM and the shared shutdown token. The token is cancelled only
/// by the daemon itself (auto-update, rollback, admin restart), so cancelling
/// it is a reliable signal of *intended restart*; the two OS signals are not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownCause {
    /// SIGINT or SIGTERM arrived from outside the process — `systemctl stop`,
    /// a `kill`, or Ctrl-C. Indistinguishable from `systemctl restart`.
    Signal,
    /// The daemon cancelled its own shutdown token to hand over to a
    /// replacement process (auto-update, rollback, or admin restart).
    Restart,
}

impl ShutdownCause {
    /// Whether this shutdown should remove the kernel state the daemon owns.
    ///
    /// See the module docs for why a restart deliberately leaves it in place.
    #[must_use]
    pub fn tears_down_runtime_state(self) -> bool {
        matches!(self, Self::Signal)
    }
}

/// Pick out the tunnel interfaces Wardnet owns from every `WireGuard` device
/// on the host.
///
/// [`TunnelInterface::list`] enumerates *all* `WireGuard` devices, including
/// ones the user created for their own purposes, so filtering by our prefix is
/// what keeps teardown from deleting someone else's tunnel.
///
/// `wg_wardin0` deliberately shares the `wg_ward` prefix (so the zone-egress
/// drop rule can match tunnels with one wildcard) but is the inbound remote-
/// access server, not an outbound tunnel. It is excluded here and torn down
/// separately through [`InboundWgInterface::tear_down_server`], which owns any
/// extra cleanup that interface needs.
#[must_use]
pub fn wardnet_tunnel_interfaces(all_interfaces: Vec<String>) -> Vec<String> {
    all_interfaces
        .into_iter()
        .filter(|name| name.starts_with(TUNNEL_INTERFACE_PREFIX) && name != INBOUND_WG_INTERFACE)
        .collect()
}

/// Remove the kernel state the daemon created: the `inet wardnet` nftables
/// table and every `wg_ward*` interface.
///
/// Every step is best-effort and logged rather than propagated: this runs on
/// the shutdown path, where failing loudly would only turn an untidy exit into
/// a failed unit. The caller has already disarmed the hardware watchdog, so a
/// slow netlink round-trip here cannot trip a reboot.
///
/// Idempotent — safe to call when some or all of the state is already gone,
/// which is what lets `wardnetd uninstall` reuse it as a belt-and-braces sweep
/// after an ungraceful kill.
///
/// Failures are both logged *and* returned. The daemon only needs the log, but
/// `wardnetd uninstall` runs before tracing is initialised, so a log-only
/// contract would let it report a clean uninstall while `table inet wardnet`
/// was still filtering traffic — the exact failure this module exists to
/// prevent.
pub async fn teardown_runtime_state(
    firewall: &Arc<dyn FirewallManager>,
    tunnels: TunnelTeardown<'_>,
    inbound_wg_interface: &Arc<dyn InboundWgInterface>,
) -> Vec<String> {
    tracing::info!("tearing down runtime state (nftables table and wireguard interfaces)");
    let mut failures = Vec::new();

    // Deletes only our named table. Never a full ruleset flush — that would
    // take Docker's and the user's own rules with it.
    if let Err(e) = firewall.destroy_wardnet_table().await {
        tracing::warn!(error = %e, "failed to destroy wardnet nftables table; continuing: {e}");
        failures.push(format!("nftables table `inet wardnet`: {e}"));
    }

    failures.extend(tunnels.tear_down().await);

    if let Err(e) = inbound_wg_interface
        .tear_down_server(INBOUND_WG_INTERFACE)
        .await
    {
        tracing::warn!(
            interface = %INBOUND_WG_INTERFACE,
            error = %e,
            "failed to tear down inbound wireguard server; continuing: {e}"
        );
        failures.push(format!("inbound server {INBOUND_WG_INTERFACE}: {e}"));
    }

    failures
}

/// How outbound tunnels get removed, which differs by caller.
///
/// The distinction is not cosmetic. Deleting a `wg_ward*` interface while the
/// database still records the tunnel as `Up` leaves the two out of step, and
/// the daemon never recovers from that on its own: the tunnel monitor's
/// `reconcile_iface_presence` flips the tunnel to `Down` and publishes
/// `TunnelDown`, whereupon `RoutingService::handle_tunnel_down` *removes* the
/// routing for every device using it and drops the route table. Nothing
/// recreates the interface — the on-demand bring-up in `apply_rule` only fires
/// for a tunnel already recorded as `Down`, and startup reconcile runs before
/// the monitor's first tick. The result would be tunnel-routed devices
/// silently falling back to direct WAN after any `systemctl stop`, until an
/// admin brought each tunnel up by hand.
pub enum TunnelTeardown<'a> {
    /// Daemon shutdown: tear down through the service so the recorded status
    /// follows the kernel. Marking tunnels `Down` is what lets the next boot's
    /// `routing.reconcile()` hit the existing on-demand bring-up in
    /// `apply_rule` and recreate each interface for the devices that need it.
    Service(&'a Arc<dyn TunnelService>),
    /// Uninstall: there is no database left to keep in step (and it may be
    /// about to be deleted), so remove the interfaces directly.
    Interface(&'a Arc<dyn TunnelInterface>),
}

impl TunnelTeardown<'_> {
    async fn tear_down(self) -> Vec<String> {
        match self {
            Self::Service(service) => Self::tear_down_via_service(service).await,
            Self::Interface(interface) => Self::tear_down_via_interface(interface).await,
        }
    }

    async fn tear_down_via_service(service: &Arc<dyn TunnelService>) -> Vec<String> {
        let mut failures = Vec::new();

        let listed = match service.list_tunnels().await {
            Ok(response) => response.tunnels,
            Err(e) => {
                tracing::warn!(error = %e, "failed to list tunnels; skipping tunnel teardown: {e}");
                return vec![format!("listing tunnels: {e}")];
            }
        };

        for tunnel in listed {
            // Already down: no interface to remove and the status is already
            // right, so the next boot will bring it up on demand if needed.
            if tunnel.status == TunnelStatus::Down {
                continue;
            }
            if let Err(e) = service
                .tear_down_internal(tunnel.id, "daemon shutdown")
                .await
            {
                tracing::warn!(
                    tunnel_id = %tunnel.id,
                    interface = %tunnel.interface_name,
                    error = %e,
                    "failed to tear down tunnel {}; continuing: {e}",
                    tunnel.interface_name
                );
                failures.push(format!("tunnel {}: {e}", tunnel.interface_name));
            }
        }

        failures
    }

    async fn tear_down_via_interface(interface: &Arc<dyn TunnelInterface>) -> Vec<String> {
        let mut failures = Vec::new();

        match interface.list().await {
            Ok(all) => {
                for name in wardnet_tunnel_interfaces(all) {
                    if let Err(e) = interface.remove(&name).await {
                        tracing::warn!(
                            interface = %name,
                            error = %e,
                            "failed to remove tunnel interface {name}; continuing: {e}"
                        );
                        failures.push(format!("tunnel interface {name}: {e}"));
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to list wireguard interfaces; skipping tunnel teardown: {e}");
                failures.push(format!("listing wireguard interfaces: {e}"));
            }
        }

        failures
    }
}
