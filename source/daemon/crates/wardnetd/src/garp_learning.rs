//! Passive learning of the upstream router's MAC for GARP failover.
//!
//! The device-discovery loop calls [`maybe_record_router_mac`] after
//! every observation. When the observed device's IP equals the
//! configured `dhcp_router_ip` we persist its MAC as
//! `garp_router_mac`, which the GARP farewell phase reads at
//! shutdown. The hook fires on every observation (not just
//! `NewDevice` / `IpChanged`) so a router replacement that keeps the
//! same IP is still picked up. The skip-if-unchanged guard avoids
//! hot-path log/DB churn.
//!
//! Two MAC keys exist by design (issue #213, decision 1):
//!
//! - `router_mac` — wizard-attested, set once via manual entry or
//!   the active ARP probe in the setup wizard. Operator-trusted,
//!   never cleared.
//! - `garp_router_mac` — passively learned from the wire. Cleared
//!   whenever `dhcp_router_ip` changes, re-populated next time
//!   discovery sees the new IP. Used only by the GARP failover
//!   sequence.

use std::sync::Arc;

use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_services::device::packet_capture::ObservedDevice;

/// If `obs.ip == dhcp_router_ip` and the stored MAC differs, write
/// the observation's MAC to `garp_router_mac`. No-op otherwise.
///
/// Errors propagate so the caller can log them; this never panics.
/// Cost on the discovery hot path: one `system_config.get` for every
/// observation (early return when the router IP doesn't match), a
/// second read plus a `set` only when the value actually changes.
pub async fn maybe_record_router_mac(
    obs: &ObservedDevice,
    repo: &Arc<dyn SystemConfigRepository>,
) -> anyhow::Result<()> {
    let Some(router_ip) = repo.get("dhcp_router_ip").await? else {
        return Ok(());
    };
    let router_ip = router_ip.trim();
    if router_ip.is_empty() || router_ip != obs.ip {
        return Ok(());
    }

    let stored = repo.get("garp_router_mac").await?.unwrap_or_default();
    if stored == obs.mac {
        return Ok(());
    }

    repo.set("garp_router_mac", &obs.mac).await?;
    tracing::info!(
        mac = %obs.mac,
        ip = %obs.ip,
        "learned upstream router MAC for GARP failover",
    );
    Ok(())
}
