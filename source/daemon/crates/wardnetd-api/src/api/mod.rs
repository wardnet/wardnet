pub mod auth;
pub mod backup;
pub mod ddns;
pub mod devices;
pub mod dhcp;
pub mod dns;
pub mod dns_capture;
pub mod dns_filter;
pub mod dns_local;
pub mod dns_log_ws;
pub mod info;
pub mod jobs;
pub mod logs_ws;
pub mod middleware;
pub mod network;
pub mod providers;
pub mod responses;
pub mod setup;
pub mod stats;
pub mod system;
pub mod tls;
pub mod tunnels;
pub mod update;

#[cfg(test)]
mod tests;

use std::time::Duration;

use crate::state::AppState;
use crate::web::static_handler;
use axum::Router;
use axum::http;
use axum::routing::get;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use wardnetd_services::auth_context::AuthContextLayer;
use wardnetd_services::request_context::RequestContextLayer;

/// Build the OpenAPI-aware router by letting each module register its own
/// handlers. Order is purely cosmetic — it controls the grouping in the
/// generated docs. Seeded with [`crate::openapi::ApiDoc`] so the merged
/// document carries the shared metadata (title, tags, security schemes).
///
/// Extracted from [`router`] so [`crate::api_doc`] can reuse the exact same
/// chain to produce a spec that includes every handler path — without it,
/// `ApiDoc::openapi()` alone only carries the static metadata.
pub(crate) fn build_openapi_router() -> OpenApiRouter<AppState> {
    let mut r = OpenApiRouter::<AppState>::with_openapi(crate::openapi::ApiDoc::openapi());
    r = auth::register(r);
    r = setup::register(r);
    r = info::register(r);
    r = devices::register(r);
    r = dns_capture::register(r);
    r = tunnels::register(r);
    r = providers::register(r);
    r = dhcp::register(r);
    r = dns::register(r);
    r = dns_filter::register(r);
    r = dns_local::register(r);
    r = ddns::register(r);
    r = tls::register(r);
    r = system::register(r);
    r = network::register(r);
    r = jobs::register(r);
    r = stats::register(r);
    r = update::register(r);
    r = backup::register(r);
    r
}

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
    // `split_for_parts` merges every handler path into the seeded `ApiDoc`
    // and returns the fully populated OpenAPI document.
    let (api_router, openapi) = build_openapi_router().split_for_parts();

    // Handler `#[utoipa::path(path = "/api/...")]` declares the full path, so
    // the generated axum router already routes under `/api/*`. WebSocket
    // endpoints cannot be modeled in OpenAPI; attach them to the generated
    // axum router as a plain route (using the full path for consistency).
    let api_router = api_router
        .route("/api/system/logs/stream", get(logs_ws::logs_ws))
        .route("/api/dns/log/stream", get(dns_log_ws::dns_log_ws));

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

    // Scalar UI: a hand-rolled HTML shell with our palette applied to Scalar's
    // sidebar CSS variables. The spec is fetched from `/api/openapi.json` and
    // the brand logo from `/api/docs/logo.png` at runtime — all three endpoints
    // share the same admin-gating extractor.
    let api_router = api_router
        .route(
            "/api/docs",
            get(|_: middleware::AdminAuth| async {
                axum::response::Html(crate::openapi::SCALAR_HTML)
            }),
        )
        .route(
            "/api/docs/logo.png",
            get(|_: middleware::AdminAuth| async {
                (
                    [(axum::http::header::CONTENT_TYPE, "image/png")],
                    crate::openapi::LOGO_PNG,
                )
            }),
        )
        .route(
            "/api/docs/scalar.js",
            get(|_: middleware::AdminAuth| async {
                // Vendored @scalar/api-reference bundle (pinned in `openapi.rs`).
                // Served from the daemon itself so /api/docs doesn't depend on
                // an external CDN — works offline and kills the supply-chain
                // surface a compromised CDN would otherwise offer inside the
                // admin session.
                (
                    [(
                        axum::http::header::CONTENT_TYPE,
                        "application/javascript; charset=utf-8",
                    )],
                    crate::openapi::SCALAR_JS,
                )
            }),
        );

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
