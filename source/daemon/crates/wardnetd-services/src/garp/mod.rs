//! Gratuitous-ARP failover hooks.
//!
//! On graceful shutdown the daemon broadcasts a *farewell* GARP that
//! re-points the LAN's `dhcp_router_ip` ARP entry at the upstream
//! router's MAC, so managed clients fall back to the home router
//! within ~1s. On startup it broadcasts a *claim* GARP that re-points
//! the same IP at the Pi's LAN-interface MAC, so clients route
//! through Wardnet again before DHCP/DNS start serving.
//!
//! Each impl owns its own 2-pulse / 500ms timing — the sequence is
//! part of the trait contract, not the caller's problem. See issue
//! #213 for the full design including the two-MAC-key invariant
//! (`router_mac` is wizard-attested; `garp_router_mac` is passively
//! learned and re-cleared whenever `dhcp_router_ip` changes).

use async_trait::async_trait;

/// Broadcasts gratuitous ARP replies on graceful shutdown / startup
/// so managed clients update their ARP caches without waiting for the
/// 1–5 minute kernel TTL.
///
/// Both methods return `Result` so the real `pnet` impl can express
/// failures (interface lookup, raw-socket open, frame send) for unit
/// tests. Call sites in `wardnetd/src/main.rs` swallow errors with a
/// `WARN` log — a GARP failure must never block the rest of the
/// shutdown / startup sequence.
#[async_trait]
pub trait GarpOps: Send + Sync {
    /// Announce that `dhcp_router_ip` is now at the upstream router's
    /// MAC (`garp_router_mac`). Called as the very first step of
    /// graceful shutdown so the stale-ARP window is minimised.
    ///
    /// Implementations short-circuit to `Ok(())` (with a log line) if
    /// either `dhcp_router_ip` or `garp_router_mac` is unset, the LAN
    /// interface can't be found, or the LAN interface has no MAC.
    async fn broadcast_farewell(&self) -> anyhow::Result<()>;

    /// Announce that `dhcp_router_ip` is now at the Pi's LAN-interface
    /// MAC. Called synchronously during bootstrap, after routing
    /// reconcile and before any user-facing runner starts, so the
    /// claim is on the wire by the time DHCP/DNS bind their sockets.
    ///
    /// Implementations short-circuit to `Ok(())` (with a log line) if
    /// `dhcp_router_ip` is unset, the LAN interface can't be found,
    /// or the LAN interface has no MAC. Unlike `broadcast_farewell`,
    /// claim has no router-MAC dependency — it broadcasts from the
    /// Pi's own MAC.
    async fn broadcast_claim(&self) -> anyhow::Result<()>;
}
