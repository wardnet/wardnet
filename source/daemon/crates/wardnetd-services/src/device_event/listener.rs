use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;
use wardnet_common::device_event::DeviceEventKind;
use wardnet_common::event::WardnetEvent;

use crate::auth_context;
use crate::device_event::service::{DeviceEventIntent, DeviceEventService, DeviceRef, IntentKind};
use crate::event::EventPublisher;

/// Turn a domain event into the timeline entry it deserves, if any.
///
/// Returns `None` for the vast majority of the bus. Adding a timeline entry is
/// a match arm here plus a catalogue entry on
/// [`DeviceEventKind`](wardnet_common::device_event::DeviceEventKind).
///
/// Deliberately pure so the mapping is unit-testable without a running bus or a
/// database — the resolution that *does* need both (a mac the event omitted,
/// and whether an arrival is a return) is the service's job.
#[must_use]
pub fn intent_from_event(event: &WardnetEvent) -> Option<DeviceEventIntent> {
    match event {
        WardnetEvent::DeviceDiscovered {
            device_id,
            mac,
            ip,
            timestamp,
            ..
        } => Some(DeviceEventIntent {
            device: DeviceRef::Id(*device_id),
            mac: Some(mac.clone()),
            kind: IntentKind::Arrival,
            details: Some(serde_json::json!({ "ip": ip })),
            at: *timestamp,
        }),

        WardnetEvent::DeviceIpChanged {
            device_id,
            mac,
            old_ip,
            new_ip,
            timestamp,
        } => Some(DeviceEventIntent {
            device: DeviceRef::Id(*device_id),
            mac: Some(mac.clone()),
            kind: IntentKind::Fixed(DeviceEventKind::IpChanged),
            details: Some(serde_json::json!({ "old_ip": old_ip, "new_ip": new_ip })),
            at: *timestamp,
        }),

        WardnetEvent::DeviceGone {
            device_id,
            mac,
            last_ip,
            timestamp,
        } => Some(DeviceEventIntent {
            device: DeviceRef::Id(*device_id),
            mac: Some(mac.clone()),
            kind: IntentKind::Fixed(DeviceEventKind::Gone),
            details: Some(serde_json::json!({ "last_ip": last_ip })),
            at: *timestamp,
        }),

        WardnetEvent::DeviceZoneChanged {
            device_id,
            old_zone_id,
            new_zone_id,
            timestamp,
        } => Some(DeviceEventIntent {
            device: DeviceRef::Id(*device_id),
            // The event carries no mac; the service resolves it.
            mac: None,
            kind: IntentKind::Fixed(DeviceEventKind::ZoneChanged),
            details: Some(serde_json::json!({
                "old_zone_id": old_zone_id.to_string(),
                "new_zone_id": new_zone_id.to_string(),
            })),
            at: *timestamp,
        }),

        WardnetEvent::RoutingRuleChanged {
            device_id,
            target,
            previous_target,
            timestamp,
            ..
        } => Some(DeviceEventIntent {
            device: DeviceRef::Id(*device_id),
            mac: None,
            kind: IntentKind::Fixed(DeviceEventKind::RoutingChanged),
            details: Some(serde_json::json!({
                "target": target,
                "previous_target": previous_target,
            })),
            at: *timestamp,
        }),

        WardnetEvent::DeviceConntrackFlushed {
            device_ip,
            reason,
            timestamp,
        } => Some(DeviceEventIntent {
            device: DeviceRef::Ip(device_ip.clone()),
            mac: None,
            kind: IntentKind::Fixed(DeviceEventKind::ConntrackFlushed),
            details: Some(serde_json::json!({ "ip": device_ip, "reason": reason })),
            at: *timestamp,
        }),

        _ => None,
    }
}

/// Watches the event bus and appends to the device timeline.
///
/// Intentionally thin — the recognise/convert logic lives in
/// [`intent_from_event`].
pub struct DeviceEventListener {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DeviceEventListener {
    /// Start the listener. `parent` roots the task's tracing span.
    ///
    /// Subscribes on the calling thread, before spawning: a broadcast receiver
    /// only sees events published after it subscribes.
    pub fn start(
        events: &Arc<dyn EventPublisher>,
        service: Arc<dyn DeviceEventService>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "device_event_listener");
        let rx = events.subscribe();
        let handle = tokio::spawn(event_loop(rx, service, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("device event listener shut down");
    }
}

async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    service: Arc<dyn DeviceEventService>,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::system();
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        let Some(intent) = intent_from_event(&event) else { continue };
                        if let Err(error) =
                            auth_context::with_context(admin_ctx.clone(), service.record(intent)).await
                        {
                            tracing::warn!(%error, "device event listener: failed to record an event");
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // The timeline is observational, so a gap is survivable —
                        // but it is a gap an admin could otherwise misread as
                        // quiet, so say so.
                        tracing::warn!(
                            skipped = n,
                            "device event listener: lagged behind event bus: skipped={n}"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("device event listener: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}
