use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

use wardnetd_services::event::EventPublisher;
use wardnetd_services::routing::RoutingService;

/// Background task that keeps the device-keyed DNS upstream snapshot
/// (#923) current.
///
/// The snapshot is derived from persisted state only (routing rules, the
/// default policy, tunnel status + DNS-override config, zone
/// allowed-targets), and every writer of that state persists **before**
/// publishing its event — so a bus listener is a race-free choke point,
/// where hand-placed refresh calls inside `RoutingService` entry points
/// would miss writers (and did: on-demand tunnel bring-up publishes only
/// `TunnelConnecting`, which the routing listener ignores). Mirrors
/// [`crate::device_snapshot_listener::DeviceSnapshotListener`]'s
/// lifecycle and burst-coalescing.
pub struct DnsDeviceSnapshotListener {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DnsDeviceSnapshotListener {
    /// Start the listener.
    ///
    /// The `parent` span parents the `dns_device_snapshot_listener` child
    /// span so all task output carries the root version field.
    pub fn start(
        events: &Arc<dyn EventPublisher>,
        routing: Arc<dyn RoutingService>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dns_device_snapshot_listener");

        let rx = events.subscribe();

        let handle = tokio::spawn(event_loop(rx, routing, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DNS device snapshot listener shut down");
    }
}

async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    routing: Arc<dyn RoutingService>,
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
                            rebuild(&*routing).await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // Missed events could include any snapshot input —
                        // rebuild unconditionally to resync with the DB.
                        tracing::warn!(skipped = n, "DNS device snapshot listener: lagged behind event bus: skipped={n}");
                        rebuild(&*routing).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("DNS device snapshot listener: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}

/// Every event that can change an input of the device-keyed snapshot:
/// the persisted rules, the default policy, a tunnel's status (all four
/// transitions — the gate is `status != Down`, so `Down → Connecting`
/// matters as much as `→ Up`), a tunnel's DNS-override config, or the
/// zone gate (a device moving zones, or a zone's allowed targets
/// changing).
fn is_relevant(event: &WardnetEvent) -> bool {
    matches!(
        event,
        WardnetEvent::RoutingRuleChanged { .. }
            | WardnetEvent::DefaultPolicyChanged { .. }
            | WardnetEvent::TunnelUp { .. }
            | WardnetEvent::TunnelDown { .. }
            | WardnetEvent::TunnelConnecting { .. }
            | WardnetEvent::TunnelReconnecting { .. }
            | WardnetEvent::TunnelDnsOverrideChanged { .. }
            | WardnetEvent::DeviceZoneChanged { .. }
            | WardnetEvent::NetworkZoneChanged { .. }
    )
}

/// Collapse an event burst into one rebuild: a tunnel deletion or a zone
/// clamp publishes one `RoutingRuleChanged` per affected device, and every
/// rebuild is a full read of the rules table — draining whatever is
/// already queued before rebuilding turns N back-to-back events into a
/// single rebuild that sees all of them (same rationale as
/// `device_snapshot_listener::drain_pending`). Dropping the drained events
/// is safe because this listener reacts to every one of them identically,
/// and irrelevant event types are ignored anyway.
fn drain_pending(rx: &mut tokio::sync::broadcast::Receiver<WardnetEvent>) {
    while matches!(
        rx.try_recv(),
        Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
    ) {}
}

async fn rebuild(routing: &dyn RoutingService) {
    let admin_ctx = AuthContext::Admin {
        admin_id: uuid::Uuid::nil(),
    };
    if let Err(e) = wardnetd_services::auth_context::with_context(
        admin_ctx,
        routing.rebuild_dns_device_upstream_snapshot(),
    )
    .await
    {
        tracing::warn!(error = %e, "failed to rebuild device-keyed DNS upstream snapshot: {e}");
    }
}
