use axum::Json;
use axum::extract::{Query, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::anomaly::{AnomalyFilter, ReevaluateSummary};
use wardnet_common::api::{ApiAnomaly, ListAnomaliesParams, ListAnomaliesResponse};

use crate::api::middleware::AdminAuth;
use crate::api::responses::AuthErrors;
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Rows returned when the caller does not say.
const DEFAULT_LIMIT: u32 = 50;

/// Register anomaly routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_anomalies))
        .routes(routes!(reevaluate_anomalies))
}

#[utoipa::path(
    get,
    path = "/api/anomalies",
    tag = "anomalies",
    description = "List anomalies: typed conditions raised by daemon components, each \
                   with a message and a remediation hint. Defaults to the ones still \
                   open, which is what the dashboard shows. Filter by `subject_id` to \
                   ask what is currently wrong with one entity — a tunnel card uses \
                   this to surface that tunnel's error. Admin only.",
    params(ListAnomaliesParams),
    responses(
        (status = 200, description = "Matching anomalies, newest first", body = ListAnomaliesResponse),
        AuthErrors,
    ),
)]
pub async fn list_anomalies(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Query(params): Query<ListAnomaliesParams>,
) -> Result<Json<ListAnomaliesResponse>, AppError> {
    let filter = AnomalyFilter {
        status: params.status.unwrap_or_default(),
        subject_id: params.subject_id,
    };
    let anomalies = state
        .anomaly_service()
        .list(filter, params.limit.unwrap_or(DEFAULT_LIMIT))
        .await?
        .into_iter()
        .map(ApiAnomaly::from)
        .collect();
    Ok(Json(ListAnomaliesResponse { anomalies }))
}

#[utoipa::path(
    post,
    path = "/api/anomalies/reevaluate",
    tag = "anomalies",
    description = "Re-check every open anomaly against the world and resolve the ones \
                   whose condition no longer holds, instead of waiting for the next \
                   scheduled pass. Resolving an anomaly that was alerted also sends the \
                   matching recovery notification. Admin only.",
    responses(
        (status = 200, description = "How many anomalies were checked and closed", body = ReevaluateSummary),
        AuthErrors,
    ),
)]
pub async fn reevaluate_anomalies(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ReevaluateSummary>, AppError> {
    let summary = state.anomaly_service().reevaluate_all().await?;
    Ok(Json(summary))
}
