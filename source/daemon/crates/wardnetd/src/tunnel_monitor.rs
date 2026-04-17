use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use wardnetd_services::tunnel::TunnelService;

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
    /// Start the monitor.
    ///
    /// The `parent` span is used as the parent for the `tunnel_monitor` child
    /// span, ensuring all log output from spawned tasks includes the root
    /// version field.
    pub fn start(
        tunnel_service: Arc<dyn TunnelService>,
        stats_interval_secs: u64,
        health_check_interval_secs: u64,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "tunnel_monitor");

        let stats_handle = tokio::spawn(
            stats_loop(tunnel_service.clone(), stats_interval_secs, cancel.clone())
                .instrument(span.clone()),
        );

        let health_handle = tokio::spawn(
            health_loop(tunnel_service, health_check_interval_secs, cancel.clone())
                .instrument(span),
        );

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

async fn stats_loop(
    tunnel_service: Arc<dyn TunnelService>,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    let mut tick = interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }

        if let Err(e) = tunnel_service.collect_stats().await {
            tracing::error!(error = %e, "stats loop: failed to collect stats: {e}");
        }
    }
}

async fn health_loop(
    tunnel_service: Arc<dyn TunnelService>,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    let mut tick = interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }

        if let Err(e) = tunnel_service.run_health_check().await {
            tracing::error!(error = %e, "health loop: failed to run health check: {e}");
        }
    }
}
