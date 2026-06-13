use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{ApiError, DnsEventItem, DnsEventsAckRequest};
use wardnet_common::event::WardnetEvent;

use crate::api::middleware::ClientIp;
use crate::state::AppState;
use wardnetd_services::error::AppError;

const TAG: &str = "devices";
const PATH_STREAM: &str = "/api/devices/me/dns-events/stream";
const PATH_ACK: &str = "/api/devices/me/dns-events/ack";

/// Register DNS events SSE routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(stream_dns_events))
        .routes(routes!(ack_dns_events))
}

#[utoipa::path(
    get,
    path = PATH_STREAM,
    tag = TAG,
    description = "SSE stream of captured DNS events for this device. \
                   Reconnect with `Last-Event-ID` to resume from the last \
                   seen row; omit the header to replay all pending events.",
    responses(
        (status = 200, description = "SSE stream of DnsEventItem (text/event-stream)"),
        (status = 404, description = "Device not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn stream_dns_events(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    // Resolve the device before opening the stream so unknown IPs get a 404.
    let response = state
        .device_service()
        .get_device_for_ip(&ip.to_string())
        .await?;
    let device_uuid: Uuid = response
        .device
        .as_ref()
        .map(|d| d.id)
        .ok_or_else(|| AppError::NotFound("device not found".to_owned()))?;
    let device_id = device_uuid.to_string();

    // Parse Last-Event-ID sent by the browser on reconnect. 0 means "start
    // from the beginning".
    let cursor: i64 = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0i64);

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(64);

    // Clone state + ids for the spawned task.
    let state_clone = state.clone();

    tokio::spawn(async move {
        // Subscribe to the event bus BEFORE the flush phase so that any
        // DnsEventInserted emitted while we are paging through pending rows
        // is buffered in the broadcast receiver rather than lost.
        let mut event_rx = state_clone.event_publisher().subscribe();

        // --- Flush phase: replay all pending rows not yet acked ----
        let mut last_flushed_id = cursor;
        loop {
            let batch = match state_clone
                .device_service()
                .fetch_pending_dns_events(&device_id, last_flushed_id, 200)
                .await
            {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(error = %e, "failed to fetch pending DNS events; closing stream");
                    return;
                }
            };

            if batch.is_empty() {
                break;
            }

            let max_id = batch.last().expect("non-empty batch has last element").id;
            for item in &batch {
                let data = match serde_json::to_string(item) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to serialize DNS event item");
                        continue;
                    }
                };
                let event = Event::default().id(item.id.to_string()).data(data);
                if tx.send(Ok(event)).await.is_err() {
                    return; // Client disconnected.
                }
            }

            last_flushed_id = max_id;

            if batch.len() < 200 {
                break; // Reached the end of pending rows.
            }
        }

        // --- Live phase: forward events from the broadcast bus ----
        loop {
            match event_rx.recv().await {
                Ok(WardnetEvent::DnsEventInserted {
                    device_id: ev_id,
                    row_id,
                    domain,
                    status,
                    captured_at,
                    ..
                }) if ev_id == device_uuid && row_id > last_flushed_id => {
                    let item = DnsEventItem {
                        id: row_id,
                        domain,
                        status,
                        captured_at,
                    };
                    let data = match serde_json::to_string(&item) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(error = %e, "failed to serialize live DNS event item");
                            continue;
                        }
                    };
                    let event = Event::default().id(row_id.to_string()).data(data);
                    if tx.send(Ok(event)).await.is_err() {
                        return; // Client disconnected.
                    }
                    last_flushed_id = row_id;
                }
                Ok(_) => {} // Event not for this device or already seen.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // Broadcast buffer overflowed. Close stream; client
                    // reconnects and the flush phase re-delivers missing rows.
                    tracing::debug!(
                        device_id = %device_uuid,
                        "DNS events broadcast lagged; closing stream to trigger client reconnect"
                    );
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::default())
        .into_response())
}

#[utoipa::path(
    post,
    path = PATH_ACK,
    tag = TAG,
    description = "Acknowledge receipt of DNS events up to and including `up_to_id`. \
                   The daemon deletes all events with id ≤ up_to_id for this device.",
    request_body = DnsEventsAckRequest,
    responses(
        (status = 204, description = "Events acknowledged and deleted"),
        (status = 404, description = "Device not found", body = ApiError),
        (status = 422, description = "Invalid request body", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn ack_dns_events(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<DnsEventsAckRequest>,
) -> Result<axum::http::StatusCode, AppError> {
    let response = state
        .device_service()
        .get_device_for_ip(&ip.to_string())
        .await?;
    let device_id = response
        .device
        .as_ref()
        .map(|d| d.id.to_string())
        .ok_or_else(|| AppError::NotFound("device not found".to_owned()))?;

    state
        .device_service()
        .ack_dns_events(&device_id, body.up_to_id)
        .await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
