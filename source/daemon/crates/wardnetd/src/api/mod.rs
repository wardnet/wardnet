pub mod auth;
pub mod devices;
pub mod info;
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
use tracing::Level;

use crate::state::AppState;
use crate::web::static_handler;

/// Build the complete application router.
///
/// Assembles all API routes under `/api/`, applies middleware (CORS, tracing),
/// and falls back to the embedded static file handler for the web UI.
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
        .route("/providers/{id}/servers", post(providers::list_servers))
        .route("/providers/{id}/setup", post(providers::setup_tunnel));

    Router::new()
        .nest("/api", api)
        .fallback(static_handler)
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
                    )
                })
                .on_response(
                    |response: &http::Response<_>, latency: Duration, span: &tracing::Span| {
                        let status = response.status().as_u16();
                        let latency_ms = latency.as_millis();
                        span.record("status", status);
                        span.record("latency_ms", latency_ms);
                        tracing::event!(
                            Level::DEBUG,
                            status = status,
                            latency_ms = latency_ms,
                            "response: status={status}, latency_ms={latency_ms}",
                        );
                    },
                ),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}
