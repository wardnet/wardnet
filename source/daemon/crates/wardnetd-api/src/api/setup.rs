use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use wardnet_common::api::{ApiError, SetupRequest, SetupResponse, SetupStatusResponse};

use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register setup wizard routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(setup_status)).routes(routes!(setup))
}

#[utoipa::path(
    get,
    path = "/api/setup/status",
    tag = "setup",
    description = "Report whether the initial setup wizard has been completed (i.e. \
                   whether any admin account exists). No authentication required — the \
                   web UI calls this before rendering to decide whether to redirect \
                   the user to the setup page.",
    responses(
        (status = 200, description = "Whether initial setup is complete", body = SetupStatusResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn setup_status(
    State(state): State<AppState>,
) -> Result<Json<SetupStatusResponse>, AppError> {
    let setup_completed = state.auth_service().is_setup_completed().await?;
    Ok(Json(SetupStatusResponse { setup_completed }))
}

#[utoipa::path(
    post,
    path = "/api/setup",
    tag = "setup",
    description = "Create the first admin account as part of the initial setup wizard. \
                   Password is hashed with Argon2id before persistence. No \
                   authentication required (because by definition no admin exists yet), \
                   but returns 409 Conflict if setup has already been completed to \
                   prevent this endpoint from being used as an admin-creation backdoor.",
    request_body = SetupRequest,
    responses(
        (status = 201, description = "Admin account created", body = SetupResponse),
        (status = 400, description = "Malformed request body", body = ApiError),
        (status = 409, description = "Setup already completed", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
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
