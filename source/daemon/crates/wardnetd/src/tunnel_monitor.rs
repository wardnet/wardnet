use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use wardnet_types::event::WardnetEvent;
use wardnet_types::tunnel::TunnelStatus;

use crate::event::EventPublisher;
use crate::repository::TunnelRepository;
use crate::wireguard::WireGuardOps;

/// Background tasks for monitoring active `WireGuard` tunnels.
///
/// Spawns two periodic loops:
/// - **Stats collection**: polls byte counters every N seconds for `Up` tunnels.
/// - **Health check**: polls `last_handshake` every N seconds for `Up` tunnels.
pub struct TunnelMonitor {
    cancel: CancellationToken,
    stats_handle: tokio::task::JoinHandle<()>,
    health_handle: tokio::task::JoinHandle<()>,
}

impl TunnelMonitor {
    /// Start the monitor with the given dependencies and intervals.
    ///
    /// Returns a `TunnelMonitor` whose background tasks run until
    /// [`shutdown`](Self::shutdown) is called.
    pub fn start(
        tunnels: Arc<dyn TunnelRepository>,
        wireguard: Arc<dyn WireGuardOps>,
        events: Arc<dyn EventPublisher>,
        stats_interval_secs: u64,
        health_check_interval_secs: u64,
    ) -> Self {
        let cancel = CancellationToken::new();

        let stats_handle = tokio::spawn(stats_loop(
            tunnels.clone(),
            wireguard.clone(),
            events,
            stats_interval_secs,
            cancel.clone(),
        ));

        let health_handle = tokio::spawn(health_loop(
            tunnels,
            wireguard,
            health_check_interval_secs,
            cancel.clone(),
        ));

        Self {
            cancel,
            stats_handle,
            health_handle,
        }
    }

    /// Cancel both background tasks and wait for them to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.stats_handle.await;
        let _ = self.health_handle.await;
        tracing::info!("tunnel monitor shut down");
    }
}

/// Periodically poll byte counters for all `Up` tunnels.
async fn stats_loop(
    tunnels: Arc<dyn TunnelRepository>,
    wireguard: Arc<dyn WireGuardOps>,
    events: Arc<dyn EventPublisher>,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    let mut tick = interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }

        let all_tunnels = match tunnels.find_all().await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "stats loop: failed to query tunnels");
                continue;
            }
        };

        let up_tunnels: Vec<_> = all_tunnels
            .into_iter()
            .filter(|t| t.status == TunnelStatus::Up)
            .collect();

        for tunnel in up_tunnels {
            let stats = match wireguard.get_stats(&tunnel.interface_name).await {
                Ok(Some(s)) => s,
                Ok(None) => continue,
                Err(e) => {
                    tracing::error!(
                        interface = %tunnel.interface_name,
                        error = %e,
                        "stats loop: failed to get stats"
                    );
                    continue;
                }
            };

            let last_handshake_str = stats.last_handshake.map(|ts| ts.to_rfc3339());

            if let Err(e) = tunnels
                .update_stats(
                    &tunnel.id.to_string(),
                    stats.bytes_tx.cast_signed(),
                    stats.bytes_rx.cast_signed(),
                    last_handshake_str.as_deref(),
                )
                .await
            {
                tracing::error!(
                    tunnel_id = %tunnel.id,
                    error = %e,
                    "stats loop: failed to update stats in database"
                );
                continue;
            }

            events.publish(WardnetEvent::TunnelStatsUpdated {
                tunnel_id: tunnel.id,
                status: TunnelStatus::Up,
                bytes_tx: stats.bytes_tx,
                bytes_rx: stats.bytes_rx,
                last_handshake: stats.last_handshake,
                timestamp: chrono::Utc::now(),
            });
        }
    }
}

/// Periodically check health of all `Up` tunnels via handshake age.
async fn health_loop(
    tunnels: Arc<dyn TunnelRepository>,
    wireguard: Arc<dyn WireGuardOps>,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    let mut tick = interval(Duration::from_secs(interval_secs));
    let stale_threshold = chrono::Duration::minutes(3);

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }

        let all_tunnels = match tunnels.find_all().await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "health loop: failed to query tunnels");
                continue;
            }
        };

        let up_tunnels: Vec<_> = all_tunnels
            .into_iter()
            .filter(|t| t.status == TunnelStatus::Up)
            .collect();

        for tunnel in up_tunnels {
            match wireguard.get_stats(&tunnel.interface_name).await {
                Ok(Some(stats)) => {
                    if let Some(last_handshake) = stats.last_handshake {
                        let age = chrono::Utc::now() - last_handshake;
                        if age > stale_threshold {
                            tracing::warn!(
                                tunnel_id = %tunnel.id,
                                interface = %tunnel.interface_name,
                                last_handshake = %last_handshake,
                                age_secs = age.num_seconds(),
                                "health check: last handshake is stale (>3 minutes)"
                            );
                        }
                    }
                }
                Ok(None) => {
                    tracing::error!(
                        tunnel_id = %tunnel.id,
                        interface = %tunnel.interface_name,
                        "health check: interface not found (may have been removed externally)"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        tunnel_id = %tunnel.id,
                        interface = %tunnel.interface_name,
                        error = %e,
                        "health check: failed to get stats"
                    );
                }
            }
        }
    }
}
