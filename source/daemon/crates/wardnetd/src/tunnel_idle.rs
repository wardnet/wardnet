use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::event::EventPublisher;
use crate::service::TunnelService;

/// Tracks device-to-tunnel assignments and manages idle teardown countdowns.
///
/// Subscribes to the event bus for `DeviceGone` and `RoutingRuleChanged` events.
/// When the last device using a tunnel departs, starts an idle countdown.
/// If no device reclaims before timeout, instructs `TunnelService` to tear it down.
///
/// Note: This is a scaffold for Milestone 1b. Full implementation requires
/// device discovery events from Milestone 1c.
pub struct IdleTunnelWatcher {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl IdleTunnelWatcher {
    /// Start the idle tunnel watcher.
    ///
    /// The `parent` span is used as the parent for the `idle_watcher` child
    /// span, ensuring all log output from the spawned task includes the root
    /// version field.
    ///
    /// Currently a stub that logs tunnel-related events. Full idle tracking
    /// logic (device-to-tunnel mapping, countdown timers) will be added in
    /// Milestone 1c when device discovery events are available.
    pub fn start(
        events: Arc<dyn EventPublisher>,
        _tunnel_service: Arc<dyn TunnelService>,
        _idle_timeout_secs: u64,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "idle_watcher");

        let handle = tokio::spawn(event_loop(events, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("idle tunnel watcher shut down");
    }
}

/// Subscribe to domain events and log tunnel-related ones.
async fn event_loop(events: Arc<dyn EventPublisher>, cancel: CancellationToken) {
    let mut rx = events.subscribe();

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        tracing::debug!(?event, "idle watcher: received event");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "idle watcher: lagged behind event bus");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("idle watcher: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}
