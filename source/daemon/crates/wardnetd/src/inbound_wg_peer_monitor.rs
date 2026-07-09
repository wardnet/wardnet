//! Background monitor for inbound `WireGuard` peer liveness (issue #810).
//!
//! Polls the inbound server interface's per-peer handshake times and drives
//! each linked device through the discovery service's peer-observation state
//! machine: a fresh handshake marks the device present (and flips its
//! `connection_mode` to `Remote`); a handshake gone stale marks it gone. This
//! is the inbound analogue of the health loop in
//! [`tunnel_monitor`](crate::tunnel_monitor), but single-loop — one poll
//! cadence is all it needs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use wardnetd_services::inbound_wg::INBOUND_WG_INTERFACE;
use wardnetd_services::{DeviceDiscoveryService, InboundWgInterface, InboundWgService};

/// A `WireGuard` handshake is considered fresh when its timestamp is within
/// this many minutes. Mirrors the `stale_threshold` in
/// `wardnetd_services::tunnel::service::TunnelServiceImpl::run_health_check`
/// (`chrono::Duration::minutes(3)`); kept as a local mirror because that value
/// is a private local there, not a shared constant. (`chrono::Duration::minutes`
/// is not a `const fn`, so the window is built from this integer at use.)
const HANDSHAKE_STALE_MINUTES: i64 = 3;

/// Background task that reconciles inbound peer handshakes into device
/// presence.
pub struct InboundWgPeerMonitor {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl InboundWgPeerMonitor {
    /// Start the monitor.
    ///
    /// The `parent` span parents the `inbound_wg_peer_monitor` child span so
    /// all spawned-task log output carries the root version field.
    pub fn start(
        inbound_wg: Arc<dyn InboundWgService>,
        interface: Arc<dyn InboundWgInterface>,
        discovery: Arc<dyn DeviceDiscoveryService>,
        interval_secs: u64,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "inbound_wg_peer_monitor");
        let handle = tokio::spawn(
            poll_loop(
                inbound_wg,
                interface,
                discovery,
                interval_secs,
                cancel.clone(),
            )
            .instrument(span),
        );
        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("inbound wireguard peer monitor shut down");
    }
}

async fn poll_loop(
    inbound_wg: Arc<dyn InboundWgService>,
    interface: Arc<dyn InboundWgInterface>,
    discovery: Arc<dyn DeviceDiscoveryService>,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    let mut tick = interval(Duration::from_secs(interval_secs));
    // device_ids that were fresh on the previous tick, for transition detection.
    let mut previously_fresh: HashSet<Uuid> = HashSet::new();

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }

        if let Err(e) = poll_once(&inbound_wg, &interface, &discovery, &mut previously_fresh).await
        {
            tracing::error!(error = %e, "inbound-wg peer monitor: poll failed: {e}");
        }
    }
}

/// One poll cycle: join peers to handshake stats, then apply fresh→present and
/// stale→gone transitions relative to `previously_fresh`.
async fn poll_once(
    inbound_wg: &Arc<dyn InboundWgService>,
    interface: &Arc<dyn InboundWgInterface>,
    discovery: &Arc<dyn DeviceDiscoveryService>,
    previously_fresh: &mut HashSet<Uuid>,
) -> anyhow::Result<()> {
    let peers = inbound_wg.list_peers_for_monitor().await?;
    if peers.is_empty() {
        // Server disabled or no peers — nothing to observe. Any device that was
        // fresh has no peer to keep it alive, so let it fall stale naturally on
        // its own (ARP timeout / next poll with a peer). Clear tracking so a
        // re-added peer starts from a clean stale→fresh transition.
        previously_fresh.clear();
        return Ok(());
    }

    let stats = interface.peer_stats(INBOUND_WG_INTERFACE).await?;
    // public_key -> last_handshake, for the join.
    let handshakes: HashMap<[u8; 32], Option<chrono::DateTime<chrono::Utc>>> = stats
        .into_iter()
        .map(|s| (s.public_key, s.last_handshake))
        .collect();

    let now = chrono::Utc::now();
    let stale_threshold = chrono::Duration::minutes(HANDSHAKE_STALE_MINUTES);
    let mut currently_fresh: HashSet<Uuid> = HashSet::new();

    for peer in &peers {
        let fresh = handshakes
            .get(&peer.public_key)
            .copied()
            .flatten()
            .is_some_and(|ts| (now - ts) <= stale_threshold);

        if fresh {
            currently_fresh.insert(peer.device_id);
            if !previously_fresh.contains(&peer.device_id) {
                // stale/absent -> fresh: the device just handshook. Full
                // observation + event on the transition only.
                if let Err(e) = discovery
                    .process_peer_observation(peer.device_id, &peer.allowed_ip)
                    .await
                {
                    tracing::warn!(
                        device_id = %peer.device_id,
                        error = %e,
                        "inbound-wg peer monitor: process_peer_observation failed for device {}: {e}",
                        peer.device_id,
                    );
                }
            } else if let Err(e) = discovery.touch_peer_presence(peer.device_id).await {
                // Already-fresh ticks: cheap keep-alive so the shared in-memory
                // `last_seen` doesn't go stale under the LAN-departure sweep.
                tracing::warn!(
                    device_id = %peer.device_id,
                    error = %e,
                    "inbound-wg peer monitor: touch_peer_presence failed for device {}: {e}",
                    peer.device_id,
                );
            }
        }
    }

    // fresh -> stale (or peer removed while fresh): mark the device gone, but
    // only if its shared liveness signal is also stale (LAN traffic may still be
    // keeping it alive). Pass the same handshake-staleness window.
    let gone_timeout =
        Duration::from_secs(u64::try_from(HANDSHAKE_STALE_MINUTES).unwrap_or(0) * 60);
    for device_id in previously_fresh.iter() {
        if !currently_fresh.contains(device_id)
            && let Err(e) = discovery.mark_peer_gone(*device_id, gone_timeout).await
        {
            tracing::warn!(
                device_id = %device_id,
                error = %e,
                "inbound-wg peer monitor: mark_peer_gone failed for device {device_id}: {e}",
            );
        }
    }

    *previously_fresh = currently_fresh;
    Ok(())
}
