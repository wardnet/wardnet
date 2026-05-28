use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::state::AppState;

/// Register health routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(health))
}

#[utoipa::path(
    get,
    path = "/v1/health",
    tag = "health",
    description = "Liveness probe. Returns 200 OK when the bridge process is running. \
                   No authentication required. Used by load balancers and uptime monitors.",
    responses(
        (status = 200, description = "Bridge is alive"),
    ),
    security(()),
)]
pub async fn health() -> StatusCode {
    StatusCode::OK
}
