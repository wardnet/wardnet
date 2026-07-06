use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{ApiError, InfoResponse};

use crate::state::AppState;

/// Register info routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(info))
}

#[utoipa::path(
    get,
    path = "/api/info",
    tag = "info",
    description = "Return the daemon version string, uptime in seconds, and premium \
                   entitlement status. Used by the web UI connection-status widget to \
                   detect that the daemon is reachable, to display which build is running, \
                   and to self-gate the mobile PWAs when not entitled. \
                   No authentication required.",
    responses(
        (status = 200, description = "Daemon version, uptime, and entitlement", body = InfoResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn info(State(state): State<AppState>) -> Json<InfoResponse> {
    let uptime = state.system_service().uptime();
    Json(InfoResponse {
        version: state.system_service().version().to_owned(),
        release_version: wardnetd_services::version::RELEASE_VERSION.to_owned(),
        uptime_seconds: uptime.as_secs(),
        entitled: state.is_entitled(),
    })
}
