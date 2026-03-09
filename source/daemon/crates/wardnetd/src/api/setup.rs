use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

use wardnet_types::api::{SetupRequest, SetupResponse, SetupStatusResponse};

use crate::error::AppError;
use crate::state::AppState;

/// GET /api/setup/status
///
/// Thin handler — returns whether the initial setup wizard has been completed.
/// No authentication required so the web UI can check before rendering.
pub async fn setup_status(
    State(state): State<AppState>,
) -> Result<Json<SetupStatusResponse>, AppError> {
    let setup_completed = state.auth_service().is_setup_completed().await?;
    Ok(Json(SetupStatusResponse { setup_completed }))
}

/// POST /api/setup
///
/// Thin handler — creates the first admin account during initial setup.
/// No authentication required. Returns 409 if setup has already been completed.
pub async fn setup(
    State(state): State<AppState>,
    Json(body): Json<SetupRequest>,
) -> Result<(StatusCode, Json<SetupResponse>), AppError> {
    state
        .auth_service()
        .setup_admin(&body.username, &body.password)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(SetupResponse {
            message: "Admin account created. You can now log in.".to_owned(),
        }),
    ))
}
