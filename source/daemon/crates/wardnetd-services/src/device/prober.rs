//! Admin-triggered TCP port probe for device identification (issue #1116).
//!
//! An answering TCP port is the strongest vendor tell available — and the only
//! identification signal that emits *unsolicited* traffic to the admin's own
//! devices. Every other signal in the subsystem is passive: DHCP options and
//! mDNS announcements are things the device already said out loud.
//!
//! Pure tokio with no netlink, so it stays in `wardnetd-services` alongside
//! `ddns/cloudflare.rs` and `cloud/tunneller_runner.rs` rather than moving out
//! to the `wardnetd` binary crate.

use std::net::IpAddr;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use tokio::net::TcpStream;

/// How long a single port is given to complete its TCP handshake. A LAN
/// handshake is sub-millisecond; a second is generous enough that a busy smart-home
/// device still answers and short enough that a silently-dropped SYN (the
/// normal case for a filtered port) does not stall the request.
const PORT_CONNECT_TIMEOUT: Duration = Duration::from_secs(1);

/// Wall-clock cap on an entire probe, whatever the port list. The per-port
/// timeout already bounds each connect and the ports run concurrently, so this
/// is the belt-and-braces "bounded total" the issue asks for: the handler is
/// synchronous, so a probe must not be able to hold a request open.
const TOTAL_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Contacts a device on a fixed set of TCP ports and reports which answered.
///
/// # Invariant
///
/// Per [ADR 0025 §5](../../../../../../docs/adr/0025-device-identification.md),
/// **nothing probes a device without a direct admin action.** There is no
/// background scan, no probe-on-discovery, and no global "identify everything"
/// toggle. A caller of this trait must be on a code path that an admin
/// explicitly triggered for one named device.
///
/// The invariant is doc-enforced. `auth_context::require_admin()` does *not*
/// enforce it: a background runner can mint
/// `AuthContext::Admin { admin_id: Uuid::nil() }` — exactly what the DHCP
/// server does today to record signals — so the auth gate would wave such a
/// caller straight through. Review against this comment is what holds the line.
///
/// The port list is a parameter rather than something the implementation
/// chooses, so the trait itself cannot widen the probe surface; production
/// callers pass [`wardnetd_data::vendor_catalog::probe_ports`] and nothing else.
#[async_trait]
pub trait DeviceProber: Send + Sync {
    /// Probe `ip` on `ports`, returning only the ports that answered, sorted.
    ///
    /// A probe never fails — it just finds nothing. A refused connection, a
    /// dropped SYN and an unreachable host are all the same answer to the only
    /// question being asked ("is something listening here?"), and none of them
    /// is an error the admin can act on.
    async fn probe(&self, ip: IpAddr, ports: &[u16]) -> Vec<u16>;
}

/// Default [`DeviceProber`]: one bounded TCP connect per port, all concurrent.
pub struct TcpDeviceProber;

#[async_trait]
impl DeviceProber for TcpDeviceProber {
    async fn probe(&self, ip: IpAddr, ports: &[u16]) -> Vec<u16> {
        let mut attempts: FuturesUnordered<_> = ports
            .iter()
            .map(|&port| async move {
                let connected =
                    tokio::time::timeout(PORT_CONNECT_TIMEOUT, TcpStream::connect((ip, port)))
                        .await
                        .is_ok_and(|result| result.is_ok());
                connected.then_some(port)
            })
            .collect();

        let mut answering = collect_until(
            tokio::time::Instant::now() + TOTAL_PROBE_TIMEOUT,
            &mut attempts,
        )
        .await;

        answering.sort_unstable();
        answering
    }
}

/// Drain `attempts` until it is exhausted or `deadline` passes, keeping every
/// port that answered before the cut-off.
///
/// Deliberately one completion at a time rather than `join_all` under a single
/// `timeout`: `join_all` is all-or-nothing, so a cap that fired would drop the
/// whole future and discard ports that had *already* answered — reporting "no
/// known ports answered" for a device that answered, indistinguishably from a
/// genuine negative. Draining means the cap can only cut off attempts still in
/// flight, which are by construction the ones waiting on a dropped SYN.
///
/// Split out from [`TcpDeviceProber::probe`] so this property is testable
/// without a socket that can be made to hang on demand.
pub(crate) async fn collect_until(
    deadline: tokio::time::Instant,
    attempts: &mut (impl futures::Stream<Item = Option<u16>> + Unpin),
) -> Vec<u16> {
    let mut answering = Vec::new();
    while let Ok(Some(result)) = tokio::time::timeout_at(deadline, attempts.next()).await {
        if let Some(port) = result {
            answering.push(port);
        }
    }
    answering
}
