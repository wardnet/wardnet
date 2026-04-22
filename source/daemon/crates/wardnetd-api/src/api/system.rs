use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::SystemStatusResponse;

use crate::api::middleware::AdminAuth;
use crate::api::responses::AuthErrors;
use crate::state::AppState;
use wardnetd_services::error::AppError;
use wardnetd_services::logging::error_notifier::ErrorEntry;

/// Register system routes (status, log download, recent errors,
/// restart) onto the given [`OpenApiRouter`]. The WebSocket log stream
/// is registered separately in [`crate::api::router`] since it cannot
/// be modeled in `OpenAPI`.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(status))
        .routes(routes!(recent_errors))
        .routes(routes!(download_logs))
        .routes(routes!(restart))
}

#[utoipa::path(
    get,
    path = "/api/system/status",
    tag = "system",
    description = "Return an aggregate snapshot of system state: daemon version, \
                   uptime, host CPU and memory usage, and counts of tracked resources \
                   (devices, tunnels, leases). Admin only.",
    responses(
        (status = 200, description = "System status (version, uptime, counts)", body = SystemStatusResponse),
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
) -> Result<Json<SystemStatusResponse>, AppError> {
    let response = state.system_service().status().await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/system/logs/download",
    tag = "system",
    description = "Download the full daemon log file as a plain-text attachment. Served \
                   with Content-Disposition: attachment so browsers prompt to save. \
                   Admin only.",
    responses(
        (status = 200, description = "Log file stream", content_type = "application/octet-stream", body = Vec<u8>),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn download_logs(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<impl IntoResponse, AppError> {
    let formatted = state.log_service().download_log_file(None).await?;

    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"wardnetd.log\"",
            ),
        ],
        Body::from(formatted),
    ))
}

/// Ask the daemon to exit so the supervisor restarts it.
///
/// Returns `204 No Content` immediately; the actual exit happens
/// ~500 ms later so the response flushes. On a Pi install systemd
/// (with `Restart=always` on `wardnetd.service`) brings the daemon
/// back up; on the dev mock the operator re-runs `make run-dev`.
#[utoipa::path(
    post,
    path = "/api/system/restart",
    tag = "system",
    description = "Ask the daemon to exit cleanly so the supervisor \
                   (systemd on a Pi install) restarts it. The response \
                   returns before the process exits; the daemon will be \
                   unreachable for a few seconds while it comes back up.",
    responses(
        (status = 204, description = "Restart scheduled"),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn restart(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<axum::http::StatusCode, AppError> {
    state.system_service().request_restart().await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// API-layer mirror of [`wardnetd_services::logging::error_notifier::ErrorEntry`] that
/// carries an `OpenAPI` schema. The service-layer type lives in a crate that does
/// not depend on `utoipa`; duplicating the small struct here keeps the schema
/// boundary aligned with the HTTP API without leaking the docs dependency into
/// the services crate.
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiErrorEntry {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub target: String,
    pub message: String,
}

impl From<ErrorEntry> for ApiErrorEntry {
    fn from(e: ErrorEntry) -> Self {
        Self {
            timestamp: e.timestamp,
            level: e.level,
            target: e.target,
            message: e.message,
        }
    }
}

/// Response for GET /api/system/errors.
#[derive(Debug, Serialize, ToSchema)]
pub struct RecentErrorsResponse {
    pub errors: Vec<ApiErrorEntry>,
}

#[utoipa::path(
    get,
    path = "/api/system/errors",
    tag = "system",
    description = "Return the most recent warnings and errors captured by the \
                   in-memory error notifier ring buffer (currently the last 15 \
                   entries). Powers the dashboard's \"recent issues\" panel. \
                   Admin only.",
    responses(
        (status = 200, description = "Recent warnings and errors", body = RecentErrorsResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn recent_errors(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Json<RecentErrorsResponse> {
    let errors = state
        .log_service()
        .get_recent_errors()
        .into_iter()
        .map(ApiErrorEntry::from)
        .collect();
    Json(RecentErrorsResponse { errors })
}
