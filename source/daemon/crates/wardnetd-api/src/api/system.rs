use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use serde::Serialize;
use wardnet_common::api::SystemStatusResponse;

use crate::api::middleware::AdminAuth;
use crate::state::AppState;
use wardnetd_services::error::AppError;
use wardnetd_services::logging::error_notifier::ErrorEntry;

/// GET /api/system/status
///
/// Thin handler — returns system status (version, uptime, counts).
/// Requires admin authentication via session cookie or API key.
pub async fn status(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<SystemStatusResponse>, AppError> {
    let response = state.system_service().status().await?;
    Ok(Json(response))
}

/// GET /api/system/logs/download
///
/// Downloads the full log file as human-readable text.
/// Requires admin authentication.
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

/// Response for GET /api/system/errors.
#[derive(Debug, Serialize)]
pub struct RecentErrorsResponse {
    pub errors: Vec<ErrorEntry>,
}

/// GET /api/system/errors
///
/// Returns the last 15 warnings and errors from the in-memory ring buffer.
/// Requires admin authentication.
pub async fn recent_errors(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Json<RecentErrorsResponse> {
    let errors = state.log_service().get_recent_errors();
    Json(RecentErrorsResponse { errors })
}
