pub mod auth;
pub mod devices;
pub mod dhcp;
pub mod dns;
pub mod info;
pub mod jobs;
pub mod logs_ws;
pub mod middleware;
pub mod providers;
pub mod responses;
pub mod setup;
pub mod system;
pub mod tunnels;
pub mod update;

#[cfg(test)]
mod tests;

use std::time::Duration;

use axum::Router;
use axum::http;
use axum::routing::get;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};

use crate::state::AppState;
use crate::web::static_handler;
use wardnetd_services::auth_context::AuthContextLayer;
use wardnetd_services::request_context::RequestContextLayer;

/// Build the complete application router.
///
/// Each module under `api/` owns its own `register(router)` function that
/// attaches its annotated handlers via `utoipa_axum::routes!`. This keeps the
/// HTTP path declared in exactly one place — the handler's `#[utoipa::path]`
/// attribute — and contains route-registration alongside the handlers instead
/// of concentrating it here.
///
/// Assembles all API routes under `/api/`, applies middleware (CORS, tracing),
/// and falls back to the embedded static file handler for the web UI.
pub fn router(state: AppState) -> Router {
    // Build the OpenAPI-aware router by letting each module register its own
    // handlers. Order is purely cosmetic — it controls the grouping in the
    // generated docs.
    //
    // Seed the router with `ApiDoc` so the merged document carries the shared
    // metadata (title, tags, security schemes) declared in `openapi.rs`.
    let mut api_router = OpenApiRouter::<AppState>::with_openapi(crate::openapi::ApiDoc::openapi());
    api_router = auth::register(api_router);
    api_router = setup::register(api_router);
    api_router = info::register(api_router);
    api_router = devices::register(api_router);
    api_router = tunnels::register(api_router);
    api_router = providers::register(api_router);
    api_router = dhcp::register(api_router);
    api_router = dns::register(api_router);
    api_router = system::register(api_router);
    api_router = jobs::register(api_router);
    api_router = update::register(api_router);

    // `split_for_parts` merges every handler path into the seeded `ApiDoc`
    // and returns the fully populated OpenAPI document.
    let (api_router, openapi) = api_router.split_for_parts();

    // Handler `#[utoipa::path(path = "/api/...")]` declares the full path, so
    // the generated axum router already routes under `/api/*`. WebSocket
    // endpoints cannot be modeled in OpenAPI; attach them to the generated
    // axum router as a plain route (using the full path for consistency).
    let api_router = api_router.route("/api/system/logs/stream", get(logs_ws::logs_ws));

    // Spec endpoint: admin-gated JSON. `AdminAuth` as an extractor ensures the
    // handler returns 401 for unauthenticated callers without any extra
    // middleware plumbing.
    let openapi_for_spec = openapi.clone();
    let api_router = api_router.route(
        "/api/openapi.json",
        get(move |_: middleware::AdminAuth| {
            let spec = openapi_for_spec.clone();
            async move { axum::Json(spec) }
        }),
    );

    // Scalar UI: a single HTML page with the spec embedded as JSON. Because
    // the spec is baked into the HTML at render time, `/api/docs` is the only
    // route Scalar registers (no sub-path asset loading to gate separately).
    // `route_layer` applies `AdminAuth` extraction to the Scalar sub-router so
    // the docs shell itself is 401 for unauthenticated callers.
    let scalar_router: Router<AppState> = Scalar::with_url("/api/docs", openapi).into();
    let scalar_router = scalar_router.route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        require_admin_mw,
    ));
    let api_router = api_router.merge(scalar_router);

    Router::new()
        .merge(api_router)
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

/// Middleware that rejects non-admin callers with 401.
///
/// Used to gate the Scalar docs router, which merges a full `Router` that can't
/// accept extractors inline. Delegates to the same session-cookie/API-key
/// logic as [`middleware::AdminAuth`] by running that extractor on the request.
async fn require_admin_mw(
    axum::extract::State(state): axum::extract::State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::extract::FromRequestParts;
    use axum::response::IntoResponse;

    let (mut parts, body) = req.into_parts();
    match middleware::AdminAuth::from_request_parts(&mut parts, &state).await {
        Ok(_) => {
            let req = axum::extract::Request::from_parts(parts, body);
            next.run(req).await
        }
        Err(err) => err.into_response(),
    }
}
