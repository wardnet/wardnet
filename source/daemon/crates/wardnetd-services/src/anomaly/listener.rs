use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::anomaly::{AnomalyReport, AnomalyType};
use wardnet_common::auth::AuthContext;
use wardnet_common::event::{TUNNEL_DOWN_INTERFACE_ABSENT, WardnetEvent};

use crate::anomaly::service::AnomalyService;
use crate::auth_context;
use crate::event::EventPublisher;

/// Turn an error-flavoured domain event into an anomaly report.
///
/// Returns `None` for events that are not admin-actionable problems — the vast
/// majority of the bus. Adding a reactive anomaly is a match arm here plus a
/// catalogue entry.
///
/// Every arm sets a `subject_id` wherever the event carries one. That is not
/// cosmetic: the subject is half the deduplication key, so an arm that omits
/// it would collapse every tunnel's failures into a single anomaly.
#[must_use]
pub fn report_from_event(event: &WardnetEvent) -> Option<AnomalyReport> {
    match event {
        WardnetEvent::TunnelStartFailed {
            tunnel_id,
            interface_name,
            error,
            timestamp,
        } => Some(
            AnomalyReport::new(
                AnomalyType::TunnelStartFailed,
                format!("Tunnel {interface_name} failed to start: {error}"),
            )
            .with_subject(tunnel_id.to_string())
            .observed_at(*timestamp),
        ),

        // A previously-up tunnel stopped handshaking. There is no error object
        // on this path — only the last handshake — so the message is
        // synthesized from how long it has been quiet.
        WardnetEvent::TunnelReconnecting {
            tunnel_id,
            interface_name,
            last_handshake,
            timestamp,
        } => {
            let detail = last_handshake.map_or_else(
                || "no handshake yet".to_owned(),
                |at| {
                    let minutes = (*timestamp - at).num_minutes().max(0);
                    format!("no handshake for {minutes} minutes")
                },
            );
            Some(
                AnomalyReport::new(
                    AnomalyType::TunnelUnhealthy,
                    format!("Tunnel {interface_name} is not passing traffic: {detail}"),
                )
                .with_subject(tunnel_id.to_string())
                .observed_at(*timestamp),
            )
        }

        // Every other `TunnelDown` reason is a deliberate tear-down and is
        // emphatically not an anomaly.
        WardnetEvent::TunnelDown {
            tunnel_id,
            interface_name,
            reason,
            timestamp,
        } if reason == TUNNEL_DOWN_INTERFACE_ABSENT => Some(
            AnomalyReport::new(
                AnomalyType::TunnelUnhealthy,
                format!("Tunnel {interface_name} lost its network interface"),
            )
            .with_subject(tunnel_id.to_string())
            .observed_at(*timestamp),
        ),

        WardnetEvent::UpdateFailed {
            target_version,
            error,
            timestamp,
            ..
        } => Some(
            AnomalyReport::new(
                AnomalyType::UpdateFailed,
                format!("Update to {target_version} failed: {error}"),
            )
            // One update anomaly at a time, box-wide: there is no per-subject
            // entity here, and the target version rides in `details` so the
            // detector can tell when the box has caught up.
            .with_details(serde_json::json!({ "target_version": target_version }))
            .observed_at(*timestamp),
        ),

        WardnetEvent::DhcpConflictDetected {
            mac,
            ip,
            details,
            timestamp,
        } => Some(
            AnomalyReport::new(
                AnomalyType::DhcpConflict,
                format!("Address conflict for {ip} (MAC {mac}): {details}"),
            )
            .with_subject(ip.clone())
            .observed_at(*timestamp),
        ),

        WardnetEvent::RouteTableLost { table, timestamp } => Some(
            AnomalyReport::new(
                AnomalyType::RouteTableLost,
                format!("Managed routing table {table} was lost"),
            )
            .with_subject(table.to_string())
            .observed_at(*timestamp),
        ),

        _ => None,
    }
}

/// The reactive half of the subsystem: watches the event bus and submits
/// anomalies for the error-flavoured events.
///
/// Intentionally thin — the recognise/convert logic lives in
/// [`report_from_event`] so it stays unit-testable without a running bus.
pub struct AnomalyListener {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl AnomalyListener {
    /// Start the listener. `parent` roots the task's tracing span.
    ///
    /// Subscribes on the calling thread, before spawning: a broadcast receiver
    /// only sees events published after it subscribes, so start this before
    /// any startup work that might raise one.
    pub fn start(
        events: &Arc<dyn EventPublisher>,
        service: Arc<dyn AnomalyService>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "anomaly_listener");
        let rx = events.subscribe();
        let handle = tokio::spawn(event_loop(rx, service, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("anomaly listener shut down");
    }
}

async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    service: Arc<dyn AnomalyService>,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        let Some(report) = report_from_event(&event) else { continue };
                        if let Err(error) =
                            auth_context::with_context(admin_ctx.clone(), service.submit(report)).await
                        {
                            tracing::warn!(%error, "anomaly listener: failed to submit an anomaly");
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "anomaly listener: lagged behind event bus: skipped={n}");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("anomaly listener: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}
