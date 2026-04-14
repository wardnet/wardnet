use axum::extract::ws::{Message, WebSocket};
use axum::extract::{State, WebSocketUpgrade};
use axum::response::IntoResponse;
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::log_broadcast::LogEntry;
use crate::state::AppState;

/// Client command sent over the WebSocket to change filters.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientCommand {
    /// Change the minimum log level filter.
    SetFilter {
        /// Minimum level: "trace", "debug", "info", "warn", "error".
        level: Option<String>,
        /// Target prefix filter (e.g. "`wardnetd::service`"). Empty = all.
        target: Option<String>,
    },
}

fn level_priority(level: &str) -> u8 {
    match level.to_uppercase().as_str() {
        "TRACE" => 0,
        "DEBUG" => 1,
        "WARN" => 3,
        "ERROR" => 4,
        // "INFO" and any unknown level default to INFO priority.
        _ => 2,
    }
}

/// GET /api/system/logs/ws
///
/// WebSocket endpoint for real-time log streaming. The client can send
/// filter commands at any time:
///
/// ```json
/// {"type": "set_filter", "level": "warn", "target": "wardnetd::service"}
/// ```
///
/// No authentication is enforced at the WS upgrade level — the connection
/// is only reachable if the client already passed the auth middleware.
pub async fn logs_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    let rx = state.log_broadcaster().subscribe();
    ws.on_upgrade(move |socket| handle_socket(socket, rx))
}

async fn handle_socket(mut socket: WebSocket, mut rx: broadcast::Receiver<LogEntry>) {
    // Default filter: info and above, all targets.
    let mut min_priority: u8 = 2; // INFO
    let mut target_prefix = String::new();

    loop {
        tokio::select! {
            // Incoming log entry from the broadcast channel.
            result = rx.recv() => {
                match result {
                    Ok(entry) => {
                        // Apply per-client filter.
                        if level_priority(&entry.level) < min_priority {
                            continue;
                        }
                        if !target_prefix.is_empty() && !entry.target.starts_with(&target_prefix) {
                            continue;
                        }

                        let Ok(json) = serde_json::to_string(&entry) else {
                            continue;
                        };

                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break; // Client disconnected.
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // Notify the client that entries were skipped.
                        let msg = serde_json::json!({
                            "type": "lagged",
                            "skipped": n,
                        });
                        let _ = socket.send(Message::Text(msg.to_string().into())).await;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }

            // Incoming message from the client (filter commands).
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<ClientCommand>(&text) {
                            match cmd {
                                ClientCommand::SetFilter { level, target } => {
                                    if let Some(ref l) = level {
                                        min_priority = level_priority(l);
                                    }
                                    if let Some(t) = target {
                                        target_prefix = t;
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {} // Ignore ping/pong/binary.
                }
            }
        }
    }
}
