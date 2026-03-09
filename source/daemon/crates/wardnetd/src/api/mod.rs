pub mod auth;
pub mod devices;
pub mod middleware;
pub mod system;
pub mod tunnels;

use axum::Router;
use axum::routing::{delete, get, post, put};
use tower_http::cors::CorsLayer;
use tower_http::trace::{DefaultOnResponse, TraceLayer};
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
        .route("/system/status", get(system::status))
        .route(
            "/tunnels",
            get(tunnels::list_tunnels).post(tunnels::create_tunnel),
        )
        .route("/tunnels/{id}", delete(tunnels::delete_tunnel));

    Router::new()
        .nest("/api", api)
        .fallback(static_handler)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::extract::Request| {
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        path = %request.uri().path(),
                        status = tracing::field::Empty,
                    )
                })
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}
