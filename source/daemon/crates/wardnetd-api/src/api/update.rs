//! `/api/update/*` — auto-update subsystem endpoints.
//!
//! All endpoints require admin authentication. Handlers are deliberately thin —
//! they call into [`UpdateService`](wardnetd_services::UpdateService), which
//! owns the state machine and background side effects.

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{
    InstallUpdateRequest, InstallUpdateResponse, RollbackResponse, UpdateCheckResponse,
    UpdateConfigRequest, UpdateConfigResponse, UpdateHistoryResponse, UpdateStatusResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register auto-update routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(status))
        .routes(routes!(check))
        .routes(routes!(install))
        .routes(routes!(rollback))
        .routes(routes!(update_config))
        .routes(routes!(history))
}

/// Query parameters for GET /api/update/history.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct HistoryQuery {
    /// Max entries to return (default 20).
    #[serde(default = "default_history_limit")]
    pub limit: u32,
}

const fn default_history_limit() -> u32 {
    20
}

#[utoipa::path(
    get,
    path = "/api/update/status",
    tag = "update",
    description = "Return the current auto-update state: currently running version, \
                   latest known release from the manifest, configured channel, and \
                   whether an install is in progress. Admin only.",
    responses(
        (status = 200, description = "Current auto-update status", body = UpdateStatusResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn status(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<UpdateStatusResponse>, AppError> {
    Ok(Json(state.update_service().status().await?))
}

#[utoipa::path(
    post,
    path = "/api/update/check",
    tag = "update",
    description = "Force an immediate refresh of the update manifest from the release \
                   server, bypassing the normal polling interval, and return whether \
                   a newer version is available on the configured channel. Admin only.",
    responses(
        (status = 200, description = "Manifest refresh result", body = UpdateCheckResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn check(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<UpdateCheckResponse>, AppError> {
    Ok(Json(state.update_service().check().await?))
}

#[utoipa::path(
    post,
    path = "/api/update/install",
    tag = "update",
    description = "Kick off an install of an available release. If the request body is \
                   omitted, the latest release on the configured channel is installed. \
                   The install runs asynchronously; poll /api/update/status for \
                   progress. Admin only.",
    request_body(content = InstallUpdateRequest, description = "Install options; if omitted, installs the latest available release"),
    responses(
        (status = 200, description = "Install initiated", body = InstallUpdateResponse),
        AuthErrors,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn install(
    State(state): State<AppState>,
    _auth: AdminAuth,
    body: Option<Json<InstallUpdateRequest>>,
) -> Result<Json<InstallUpdateResponse>, AppError> {
    let req = body.map(|b| b.0).unwrap_or_default();
    Ok(Json(state.update_service().install(req).await?))
}

#[utoipa::path(
    post,
    path = "/api/update/rollback",
    tag = "update",
    description = "Roll back to the previous binary that was swapped aside as \
                   `<live>.old` during the last successful install. Used to recover \
                   from a bad release without manual SSH intervention. Admin only.",
    responses(
        (status = 200, description = "Rollback initiated", body = RollbackResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn rollback(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<RollbackResponse>, AppError> {
    Ok(Json(state.update_service().rollback().await?))
}

#[utoipa::path(
    put,
    path = "/api/update/config",
    tag = "update",
    description = "Update auto-update settings: enable or disable automatic installs \
                   and switch between release channels (e.g. stable vs. beta). Admin \
                   only.",
    request_body = UpdateConfigRequest,
    responses(
        (status = 200, description = "Updated auto-update configuration", body = UpdateConfigResponse),
        AuthErrors,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn update_config(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<UpdateConfigRequest>,
) -> Result<Json<UpdateConfigResponse>, AppError> {
    Ok(Json(state.update_service().update_config(body).await?))
}

#[utoipa::path(
    get,
    path = "/api/update/history",
    tag = "update",
    description = "List recent install and rollback attempts, including timestamp, \
                   source and target versions, outcome, and any error message. The \
                   `limit` query parameter caps the number of entries returned \
                   (default 20). Admin only.",
    params(HistoryQuery),
    responses(
        (status = 200, description = "Update history", body = UpdateHistoryResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn history(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<UpdateHistoryResponse>, AppError> {
    Ok(Json(state.update_service().history(query.limit).await?))
}
