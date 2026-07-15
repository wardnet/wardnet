use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::event::WardnetEvent;

use wardnetd_services::device::DeviceIpSnapshot;
use wardnetd_services::event::EventPublisher;

/// Background task that keeps the DNS hot-path IP → device-id snapshot
/// current.
///
/// Subscribes to the event bus and triggers a full [`DeviceIpSnapshot`]
/// rebuild on every event that can move an IP between devices. Mirrors
/// [`crate::routing_listener::RoutingListener`]'s lifecycle.
pub struct DeviceSnapshotListener {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DeviceSnapshotListener {
    /// Start the listener.
    ///
    /// The `parent` span parents the `device_snapshot_listener` child span so
    /// all task output carries the root version field.
    pub fn start(
        events: &Arc<dyn EventPublisher>,
        snapshot: Arc<DeviceIpSnapshot>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "device_snapshot_listener");

        let rx = events.subscribe();

        let handle = tokio::spawn(event_loop(rx, snapshot, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("device snapshot listener shut down");
    }
}

async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    snapshot: Arc<DeviceIpSnapshot>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        if is_relevant(&event) {
                            drain_pending(&mut rx);
                            rebuild(&snapshot).await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // Missed events could include IP moves — rebuild
                        // unconditionally to resync with the devices table.
                        tracing::warn!(skipped = n, "device snapshot listener: lagged behind event bus: skipped={n}");
                        rebuild(&snapshot).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("device snapshot listener: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}

fn is_relevant(event: &WardnetEvent) -> bool {
    matches!(
        event,
        WardnetEvent::DeviceDiscovered { .. }
            | WardnetEvent::DeviceIpChanged { .. }
            | WardnetEvent::DeviceGone { .. }
    )
}

/// Collapse an event burst into one rebuild: a departure sweep or discovery
/// scan publishes one event per device, and every rebuild is a full read of
/// the devices table — draining whatever is already queued before rebuilding
/// turns N back-to-back events into a single rebuild that sees all of them.
/// (Same burst-collapsing rationale as the zone enforcer's FIX 6.) Dropping
/// the drained events is safe because this listener reacts to every one of
/// them identically, and irrelevant event types are ignored anyway.
fn drain_pending(rx: &mut tokio::sync::broadcast::Receiver<WardnetEvent>) {
    while matches!(
        rx.try_recv(),
        Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
    ) {}
}

async fn rebuild(snapshot: &DeviceIpSnapshot) {
    if let Err(e) = snapshot.rebuild().await {
        tracing::warn!(error = %e, "failed to rebuild device IP snapshot: {e}");
    }
}
