pub mod access_requests;
pub mod anomalies;
pub mod auth;
pub mod backup;
pub mod ddns;
pub mod devices;
pub mod dhcp;
pub mod dns;
pub mod dns_capture;
pub mod dns_events;
pub mod dns_filter;
pub mod dns_local;
pub mod dns_log_ws;
pub mod health;
pub mod inbound_wg;
pub mod info;
pub mod jobs;
pub mod logs_ws;
pub mod middleware;
pub mod network;
pub mod network_zone;
pub mod private_dns;
pub mod providers;
pub mod push;
pub mod responses;
pub mod routing_profiles;
pub mod setup;
pub mod stats;
pub mod system;
pub mod tls;
pub mod tunnels;
pub mod update;
pub mod users;
pub mod zone_exception;

#[cfg(test)]
mod tests;

use std::any::Any;
use std::time::Duration;

use crate::state::AppState;
use crate::web::static_handler;
use axum::Json;
use axum::Router;
use axum::http;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use wardnet_common::api::ApiError;
use wardnetd_services::auth_context::AuthContextLayer;
use wardnetd_services::request_context;
use wardnetd_services::request_context::RequestContextLayer;

/// Convert a caught handler panic into a `500` response — the panic-isolation
/// boundary for the whole HTTP stack (see [`catch_panic_layer`]).
///
/// Logs the panic at `error` level so it reaches structured logs / Loki (the
/// default panic hook only writes to stderr, which is how the equivalent
/// wardnet-cloud incident produced zero log lines), then returns the same
/// `ApiError` JSON shape as [`wardnetd_services::error::AppError`] so clients
/// see a consistent body. The panic message is deliberately **not** echoed in
/// the response — it can carry internal detail — but it is always logged.
// The by-value `Box` is dictated by `CatchPanicLayer::custom`'s handler
// signature; we only need to borrow it to extract the message.
#[allow(clippy::needless_pass_by_value)]
fn handle_panic(panic: Box<dyn Any + Send + 'static>) -> Response {
    let detail = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&'static str>().copied())
        .unwrap_or("unknown panic");

    tracing::error!(
        panic = %detail,
        "request handler panicked; isolated as 500 (listener kept alive): {detail}"
    );

    let body = ApiError {
        error: "internal server error".to_owned(),
        detail: None,
        request_id: request_context::current_request_id(),
    };
    (StatusCode::INTERNAL_SERVER_ERROR, Json(body)).into_response()
}

/// Panic-isolation layer for the HTTP stack. A panic in any handler (or in the
/// middleware it wraps) is caught and turned into a logged `500` by
/// [`handle_panic`], so a single bad request can never unwind the connection
/// task and take the listener down. Placed *inside* the [`TraceLayer`] so the
/// resulting `500` is still recorded by the request-tracing span.
///
/// Exposed so other served routers in the daemon (e.g. the `:80` HTTP→HTTPS
/// redirect listener) get the same panic isolation — every listener must be
/// covered, not just the main API.
#[must_use]
pub fn catch_panic_layer() -> CatchPanicLayer<fn(Box<dyn Any + Send + 'static>) -> Response> {
    CatchPanicLayer::custom(handle_panic as fn(Box<dyn Any + Send + 'static>) -> Response)
}

/// Build the `OpenAPI`-aware router by letting each module register its own
/// handlers. Order is purely cosmetic — it controls the grouping in the
/// generated docs. Seeded with [`crate::openapi::ApiDoc`] so the merged
/// document carries the shared metadata (title, tags, security schemes).
///
/// Extracted from [`router`] so [`crate::api_doc`] can reuse the exact same
/// chain to produce a spec that includes every handler path — without it,
/// `ApiDoc::openapi()` alone only carries the static metadata.
pub(crate) fn build_openapi_router() -> OpenApiRouter<AppState> {
    let mut r = OpenApiRouter::<AppState>::with_openapi(crate::openapi::ApiDoc::openapi());
    r = anomalies::register(r);
    r = auth::register(r);
    r = users::register(r);
    r = setup::register(r);
    r = info::register(r);
    r = health::register(r);
    r = devices::register(r);
    r = dns_capture::register(r);
    r = dns_events::register(r);
    r = tunnels::register(r);
    r = inbound_wg::register(r);
    r = private_dns::register(r);
    r = providers::register(r);
    r = dhcp::register(r);
    r = dns::register(r);
    r = dns_filter::register(r);
    r = dns_local::register(r);
    r = ddns::register(r);
    r = tls::register(r);
    r = system::register(r);
    r = routing_profiles::register(r);
    r = network::register(r);
    r = network_zone::register(r);
    r = zone_exception::register(r);
    r = jobs::register(r);
    r = stats::register(r);
    r = access_requests::register(r);
    r = push::register(r);
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
/// Assembles all API routes under `/api/`, applies middleware (tracing,
/// request/auth context), and falls back to the embedded static file handler
/// for the web UI.
///
/// Note the deliberate absence of a CORS layer: every web surface the daemon
/// serves (the user PWA at `/app/`, the admin mobile PWA at `/admin-app/`, and
/// the desktop admin site at `/admin/`) is embedded and served from this same
/// origin, so no legitimate caller is cross-origin. Emitting any
/// `Access-Control-Allow-Origin` here would be actively harmful: the
/// self-service endpoints authenticate the caller by TCP peer IP
/// ([`AuthContext::Device`](wardnet_common::auth::AuthContext), ambient
/// authority that `SameSite` cookies cannot protect), so a permissive CORS policy
/// let any web page a LAN user visited drive authenticated state changes and
/// read responses cross-origin (CWE-352). With no CORS layer the browser's
/// same-origin policy blocks cross-origin reads and fails the preflight for the
/// non-simple `PUT`/`PATCH`/`DELETE`/JSON-`POST` requests those endpoints use.
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

    // Spec endpoint: admin-gated JSON. These closures call no service, so there
    // is nowhere to put `require_admin()` — `AdminOnly` is the guard itself.
    // `SessionAuth` would not do: it admits `member` callers, and this endpoint
    // returns the whole admin API surface.
    let openapi_for_spec = openapi.clone();
    let api_router = api_router.route(
        "/api/openapi.json",
        get(move |_: middleware::AdminOnly| {
            let spec = openapi_for_spec.clone();
            async move { axum::Json(spec) }
        }),
    );

    // Scalar UI: a hand-rolled HTML shell with our palette applied to Scalar's
    // sidebar CSS variables. The spec is fetched from `/api/openapi.json` and
    // the brand logo from `/api/docs/logo.svg` at runtime — all three endpoints
    // share the same `AdminOnly` extractor, for the reason above.
    let api_router = api_router
        .route(
            "/api/docs",
            get(|_: middleware::AdminOnly| async {
                axum::response::Html(crate::openapi::SCALAR_HTML)
            }),
        )
        .route(
            "/api/docs/logo.svg",
            get(|_: middleware::AdminOnly| async {
                // Unlike the PNG it replaced, SVG is an active document type:
                // scripts inside it would run in the admin origin if the URL
                // is opened directly. The asset is vendored and reviewed, but
                // the CSP neuters any script/fetch a future re-vendor might
                // smuggle in (it doesn't affect <img> usage in the docs page).
                (
                    [
                        (axum::http::header::CONTENT_TYPE, "image/svg+xml"),
                        (
                            axum::http::header::CONTENT_SECURITY_POLICY,
                            "default-src 'none'; style-src 'unsafe-inline'; sandbox",
                        ),
                    ],
                    crate::openapi::LOGO_SVG,
                )
            }),
        )
        .route(
            "/api/docs/scalar.js",
            get(|_: middleware::AdminOnly| async {
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
        // Panic isolation: catch any handler/middleware panic and turn it into a
        // logged 500 instead of letting it unwind the connection task (which can
        // take the listener down). Inside `TraceLayer` so the 500 is still
        // traced; outside our auth/context middleware so their panics are caught
        // too.
        .layer(catch_panic_layer())
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
        // No CORS layer, intentionally — see this function's doc comment. Every
        // served web surface is same-origin, and the IP-authenticated
        // self-service endpoints must never answer a cross-origin preflight or
        // emit `Access-Control-Allow-Origin`, or a hostile web page could forge
        // authenticated requests / read responses cross-origin (CWE-352).
        .with_state(state)
}
