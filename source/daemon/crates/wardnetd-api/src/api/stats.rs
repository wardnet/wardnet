use axum::Json;
use axum::extract::{Query, State};
use axum_extra::extract::Query as ExtraQuery;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::stats::{StatsQuery, StatsQueryResponse, StatsTopQuery, StatsTopResponse};

use crate::api::middleware::SessionAuth;
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
    description = "Query a time-series metric. Pass either `metric=<name>` for a \
                   single series, or repeat `metrics=<name>` to fetch multiple \
                   series in one round-trip — both share the same time range, \
                   bucket, and label filter. Admin only.",
    params(StatsQuery),
    responses(
        (status = 200, description = "Stats query result", body = StatsQueryResponse),
        AuthErrors,
    ),
)]
pub async fn query(
    State(state): State<AppState>,
    _auth: SessionAuth,
    // `axum_extra::extract::Query` (serde_html_form) supports repeated
    // keys for `Vec<String>` — the standard axum `Query` extractor
    // (`serde_urlencoded`) does not.
    ExtraQuery(body): ExtraQuery<StatsQuery>,
) -> Result<Json<StatsQueryResponse>, AppError> {
    let response = state.stats_service().query(body).await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/stats/top",
    tag = "stats",
    description = "Return the top-N label values for a metric. Admin only.",
    params(StatsTopQuery),
    responses(
        (status = 200, description = "Top-N query result", body = StatsTopResponse),
        AuthErrors,
    ),
)]
pub async fn top(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Query(body): Query<StatsTopQuery>,
) -> Result<Json<StatsTopResponse>, AppError> {
    let response = state.stats_service().top(body).await?;
    Ok(Json(response))
}
