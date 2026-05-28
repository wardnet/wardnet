pub mod acme;
pub mod challenge;
pub mod deregister;
pub mod health;
pub mod ip;
pub mod names;
pub mod register;
pub(crate) mod validation;

use axum::Router;
use axum::middleware;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::auth::middleware::auth_layer;
use crate::error::ErrorBody;
use crate::state::AppState;

/// `OpenAPI` document metadata.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Wardnet Bridge API",
        description = "DDNS + ACME credential proxy for wardnet installations.",
        version = "0.1.0",
    ),
    tags(
        (name = "health",   description = "Liveness probes"),
        (name = "installs", description = "Registration, IP updates, and ACME challenge lifecycle"),
    ),
    components(schemas(ErrorBody)),
)]
struct ApiDoc;

/// Build the OpenAPI-aware router with all routes registered.
fn build_openapi_router() -> OpenApiRouter<AppState> {
    let mut r = OpenApiRouter::<AppState>::with_openapi(ApiDoc::openapi());
    r = health::register(r);
    r = challenge::register(r);
    r = register::register(r);
    r = names::register(r);
    r = ip::register(r);
    r = acme::register(r);
    r = deregister::register(r);
    r
}

/// Build the complete Axum [`Router`] with middleware applied.
pub fn router(state: AppState) -> Router {
    let (api_router, _openapi) = build_openapi_router().split_for_parts();

    api_router
        .layer(middleware::from_fn_with_state(state.clone(), auth_layer))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
