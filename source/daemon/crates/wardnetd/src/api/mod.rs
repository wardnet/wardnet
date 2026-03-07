pub mod auth;
pub mod devices;
pub mod middleware;
pub mod system;

use axum::Router;
use axum::routing::{get, post, put};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;
use crate::web::static_handler;

/// Build the complete application router.
///
/// Assembles all API routes under `/api/`, applies middleware (CORS, tracing),
/// and falls back to the embedded static file handler for the web UI.
pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/auth/login", post(auth::login))
        .route("/devices/me", get(devices::get_me))
        .route("/devices/me/rule", put(devices::set_my_rule))
        .route("/system/status", get(system::status));

    Router::new()
        .nest("/api", api)
        .fallback(static_handler)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
