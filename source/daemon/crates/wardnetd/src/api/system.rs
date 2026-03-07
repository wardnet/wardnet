use axum::Json;
use axum::extract::State;
use wardnet_types::api::SystemStatusResponse;

use crate::api::middleware::AdminAuth;
use crate::error::AppError;
use crate::state::AppState;

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
