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
pub(crate) struct ClientFilter {
    pub domain: String,
    pub client_ip: String,
    pub results: Vec<String>,
}

impl ClientFilter {
    pub(crate) fn matches(&self, event: &QueryLogEvent) -> bool {
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

    /// Parse a JSON `SetFilter` command and apply each `Some` field.
    /// Returns true when the JSON parsed and was applied. Extracted out
    /// of `handle_socket` for direct unit-testing.
    pub(crate) fn apply_command_json(&mut self, json: &str) -> bool {
        let Ok(cmd) = serde_json::from_str::<ClientCommand>(json) else {
            return false;
        };
        let ClientCommand::SetFilter {
            domain,
            client_ip,
            results,
        } = cmd
        else {
            return false;
        };
        if let Some(d) = domain {
            self.domain = d;
        }
        if let Some(ip) = client_ip {
            self.client_ip = ip;
        }
        if let Some(r) = results {
            self.results = r;
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
                        filter.apply_command_json(&text);
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn event(domain: &str, ip: &str, result: &str) -> QueryLogEvent {
        QueryLogEvent {
            timestamp: "2026-05-05T00:00:00Z".to_owned(),
            client_ip: ip.to_owned(),
            domain: domain.to_owned(),
            query_type: "A".to_owned(),
            result: result.to_owned(),
            upstream: None,
            latency_ms: 0.0,
            device_id: None,
        }
    }

    #[test]
    fn empty_filter_matches_anything() {
        let f = ClientFilter::default();
        assert!(f.matches(&event("example.com", "10.0.0.1", "forwarded")));
    }

    #[test]
    fn domain_filter_uses_substring() {
        let f = ClientFilter {
            domain: "ads".to_owned(),
            ..Default::default()
        };
        assert!(f.matches(&event("ads.tracker.io", "10.0.0.1", "blocked")));
        assert!(!f.matches(&event("example.com", "10.0.0.1", "forwarded")));
    }

    #[test]
    fn client_ip_filter_is_exact() {
        let f = ClientFilter {
            client_ip: "10.0.0.5".to_owned(),
            ..Default::default()
        };
        assert!(f.matches(&event("a.com", "10.0.0.5", "forwarded")));
        assert!(!f.matches(&event("a.com", "10.0.0.6", "forwarded")));
    }

    #[test]
    fn results_filter_is_any_of() {
        let f = ClientFilter {
            results: vec!["blocked".to_owned(), "rewritten".to_owned()],
            ..Default::default()
        };
        assert!(f.matches(&event("a.com", "10.0.0.5", "blocked")));
        assert!(f.matches(&event("a.com", "10.0.0.5", "rewritten")));
        assert!(!f.matches(&event("a.com", "10.0.0.5", "forwarded")));
    }

    #[test]
    fn all_filters_combine_with_and() {
        let f = ClientFilter {
            domain: "tracker".to_owned(),
            client_ip: "10.0.0.5".to_owned(),
            results: vec!["blocked".to_owned()],
        };
        assert!(f.matches(&event("ads.tracker.io", "10.0.0.5", "blocked")));
        // Domain miss
        assert!(!f.matches(&event("ok.com", "10.0.0.5", "blocked")));
        // IP miss
        assert!(!f.matches(&event("ads.tracker.io", "10.0.0.6", "blocked")));
        // Result miss
        assert!(!f.matches(&event("ads.tracker.io", "10.0.0.5", "forwarded")));
    }

    #[test]
    fn apply_command_json_set_filter_updates_fields() {
        let mut f = ClientFilter::default();
        let ok = f.apply_command_json(
            r#"{"type":"set_filter","domain":"ads","client_ip":"10.0.0.5","results":["blocked"]}"#,
        );
        assert!(ok);
        assert_eq!(f.domain, "ads");
        assert_eq!(f.client_ip, "10.0.0.5");
        assert_eq!(f.results, vec!["blocked".to_owned()]);
    }

    #[test]
    fn apply_command_json_partial_only_overwrites_some_fields() {
        let mut f = ClientFilter {
            domain: "existing".to_owned(),
            client_ip: "10.0.0.5".to_owned(),
            ..Default::default()
        };
        let ok = f.apply_command_json(r#"{"type":"set_filter","domain":"new"}"#);
        assert!(ok);
        assert_eq!(f.domain, "new");
        // Untouched.
        assert_eq!(f.client_ip, "10.0.0.5");
    }

    #[test]
    fn apply_command_json_invalid_json_is_ignored() {
        let mut f = ClientFilter::default();
        let ok = f.apply_command_json("not json");
        assert!(!ok);
    }

    #[test]
    fn apply_command_json_unknown_command_is_ignored() {
        let mut f = ClientFilter::default();
        // The `Unknown` variant matches any other tag; this exercises
        // the early-return in `apply_command_json`.
        let ok = f.apply_command_json(r#"{"type":"something_else"}"#);
        assert!(!ok);
    }
}
