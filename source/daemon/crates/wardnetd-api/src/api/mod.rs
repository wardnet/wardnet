pub mod auth;
pub mod devices;
pub mod dhcp;
pub mod dns;
pub mod info;
pub mod logs_ws;
pub mod middleware;
pub mod providers;
pub mod setup;
pub mod system;
pub mod tunnels;

#[cfg(test)]
mod tests;

use std::time::Duration;

use axum::Router;
use axum::http;
use axum::routing::{delete, get, post, put};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;
use crate::web::static_handler;
use wardnetd_services::auth_context::AuthContextLayer;
use wardnetd_services::request_context::RequestContextLayer;

/// Build the complete application router.
///
/// Assembles all API routes under `/api/`, applies middleware (CORS, tracing),
/// and falls back to the embedded static file handler for the web UI.
#[allow(clippy::too_many_lines)]
pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/login", post(auth::login))
        .route("/devices", get(devices::list_devices))
        .route("/devices/me", get(devices::get_me))
        .route("/devices/me/rule", put(devices::set_my_rule))
        .route(
            "/devices/{id}",
            get(devices::get_device).put(devices::update_device),
        )
        .route("/info", get(info::info))
        .route("/setup/status", get(setup::setup_status))
        .route("/setup", post(setup::setup))
        .route("/system/status", get(system::status))
        .route("/system/errors", get(system::recent_errors))
        .route("/system/logs/stream", get(logs_ws::logs_ws))
        .route("/system/logs/download", get(system::download_logs))
        .route(
            "/tunnels",
            get(tunnels::list_tunnels).post(tunnels::create_tunnel),
        )
        .route("/tunnels/{id}", delete(tunnels::delete_tunnel))
        .route("/providers", get(providers::list_providers))
        .route(
            "/providers/{id}/validate",
            post(providers::validate_credentials),
        )
        .route("/providers/{id}/countries", get(providers::list_countries))
        .route("/providers/{id}/servers", post(providers::list_servers))
        .route("/providers/{id}/setup", post(providers::setup_tunnel))
        .route(
            "/dhcp/config",
            get(dhcp::get_config).put(dhcp::update_config),
        )
        .route("/dhcp/config/toggle", post(dhcp::toggle))
        .route("/dhcp/leases", get(dhcp::list_leases))
        .route("/dhcp/leases/{id}", delete(dhcp::revoke_lease))
        .route(
            "/dhcp/reservations",
            get(dhcp::list_reservations).post(dhcp::create_reservation),
        )
        .route("/dhcp/reservations/{id}", delete(dhcp::delete_reservation))
        .route("/dhcp/status", get(dhcp::status))
        .route("/dns/config", get(dns::get_config).put(dns::update_config))
        .route("/dns/config/toggle", post(dns::toggle))
        .route("/dns/status", get(dns::status))
        .route("/dns/cache/flush", post(dns::flush_cache))
        .route(
            "/dns/blocklists",
            get(dns::list_blocklists).post(dns::create_blocklist),
        )
        .route(
            "/dns/blocklists/{id}",
            put(dns::update_blocklist).delete(dns::delete_blocklist),
        )
        .route(
            "/dns/blocklists/{id}/update",
            post(dns::update_blocklist_now),
        )
        .route(
            "/dns/allowlist",
            get(dns::list_allowlist).post(dns::create_allowlist_entry),
        )
        .route("/dns/allowlist/{id}", delete(dns::delete_allowlist_entry))
        .route(
            "/dns/rules",
            get(dns::list_filter_rules).post(dns::create_filter_rule),
        )
        .route(
            "/dns/rules/{id}",
            put(dns::update_filter_rule).delete(dns::delete_filter_rule),
        );

    Router::new()
        .nest("/api", api)
        .fallback(static_handler)
        .layer(AuthContextLayer)
        .layer(RequestContextLayer)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::resolve_auth_context,
        ))
        .layer(axum::middleware::from_fn(
            middleware::inject_request_context,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::extract::Request| {
                    let method = request.method();
                    let path = request.uri().path();
                    let content_length = request
                        .headers()
                        .get(http::header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
                    tracing::info_span!(
                        "http_request",
                        method = %method,
                        path = %path,
                        content_length = %content_length,
                        status = tracing::field::Empty,
                        latency_ms = tracing::field::Empty,
                        request_id = tracing::field::Empty,
                        correlation_id = tracing::field::Empty,
                    )
                })
                .on_response(
                    |response: &http::Response<_>, latency: Duration, span: &tracing::Span| {
                        span.record("status", response.status().as_u16());
                        span.record("latency_ms", latency.as_millis());
                        tracing::debug!("response");
                    },
                ),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}
