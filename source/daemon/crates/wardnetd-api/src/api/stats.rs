use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::stats::{StatsQuery, StatsQueryResponse, StatsTopQuery, StatsTopResponse};

use crate::api::middleware::AdminAuth;
use crate::api::responses::AuthErrors;
use crate::state::AppState;
use wardnetd_services::error::AppError;

pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(query)).routes(routes!(top))
}

#[utoipa::path(
    get,
    path = "/api/stats",
    tag = "stats",
    description = "Query a time-series metric. Admin only.",
    request_body = StatsQuery,
    responses(
        (status = 200, description = "Stats query result", body = StatsQueryResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn query(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<StatsQuery>,
) -> Result<Json<StatsQueryResponse>, AppError> {
    let response = state.stats_service().query(body).await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/stats/top",
    tag = "stats",
    description = "Return the top-N label values for a metric. Admin only.",
    request_body = StatsTopQuery,
    responses(
        (status = 200, description = "Top-N query result", body = StatsTopResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn top(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<StatsTopQuery>,
) -> Result<Json<StatsTopResponse>, AppError> {
    let response = state.stats_service().top(body).await?;
    Ok(Json(response))
}
