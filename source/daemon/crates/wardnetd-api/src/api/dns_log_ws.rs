//! WebSocket endpoint for the live DNS query stream.
//!
//! Unlike `logs_ws`, this handler **explicitly takes** an [`AdminAuth`]
//! extractor — un-authenticated callers get 401 *before* the WS upgrade
//! happens. This is the gate `/api/dns/log/stream` was designed against
//! (see issue #321 for the existing un-gated `system/logs/stream` bug).

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use serde::Deserialize;
use tokio::sync::broadcast;
use wardnet_common::api::QueryLogEvent;

use crate::api::middleware::AdminAuth;
use crate::state::AppState;

/// Client command sent over the WebSocket to filter the broadcast stream
/// before it ships to the browser. Filters apply incrementally — the
/// server stores the latest values and applies them to subsequent events.
#[derive(Debug, Default, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientCommand {
    /// Update the per-client filter. Any field set to `Some` overwrites
    /// the previous value; `None` leaves it unchanged. To clear a value
    /// pass an empty string for `domain`/`client_ip` or an empty array
    /// for `results`.
    SetFilter {
        domain: Option<String>,
        client_ip: Option<String>,
        results: Option<Vec<String>>,
    },
    #[default]
    #[serde(other)]
    Unknown,
}

/// `GET /api/dns/log/stream` — admin-gated WS upgrade. The `_auth`
/// extractor refuses the upgrade when the caller has no admin session,
/// so unauthenticated clients see 401 instead of an upgrade.
pub async fn dns_log_ws(
    State(state): State<AppState>,
    _auth: AdminAuth,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let rx = match state.dns_service().subscribe_query_stream() {
        Ok(rx) => rx,
        Err(e) => {
            tracing::warn!(error = %e, "DNS log stream subscribe failed: {e}");
            return e.into_response();
        }
    };
    ws.on_upgrade(move |socket| handle_socket(socket, rx))
}

#[derive(Default)]
struct ClientFilter {
    domain: String,
    client_ip: String,
    results: Vec<String>,
}

impl ClientFilter {
    fn matches(&self, event: &QueryLogEvent) -> bool {
        if !self.domain.is_empty() && !event.domain.contains(&self.domain) {
            return false;
        }
        if !self.client_ip.is_empty() && event.client_ip != self.client_ip {
            return false;
        }
        if !self.results.is_empty() && !self.results.iter().any(|r| r == &event.result) {
            return false;
        }
        true
    }
}

async fn handle_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<QueryLogEvent>) {
    let mut filter = ClientFilter::default();

    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        if !filter.matches(&event) {
                            continue;
                        }
                        let Ok(json) = serde_json::to_string(&event) else { continue };
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        let msg = serde_json::json!({
                            "type": "lagged",
                            "skipped": n,
                        });
                        let _ = socket.send(Message::Text(msg.to_string().into())).await;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(ClientCommand::SetFilter { domain, client_ip, results }) =
                            serde_json::from_str::<ClientCommand>(&text)
                        {
                            if let Some(d) = domain {
                                filter.domain = d;
                            }
                            if let Some(ip) = client_ip {
                                filter.client_ip = ip;
                            }
                            if let Some(r) = results {
                                filter.results = r;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
