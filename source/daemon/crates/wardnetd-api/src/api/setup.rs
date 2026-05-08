use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use wardnet_common::api::{
    AdvanceWizardRequest, AdvanceWizardResponse, ApiError, SetupRequest, SetupResponse,
    SetupStatusResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::AuthErrors;
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register setup wizard routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(setup_status))
        .routes(routes!(setup))
        .routes(routes!(setup_advance))
}

#[utoipa::path(
    get,
    path = "/api/setup/status",
    tag = "setup",
    description = "Report the current setup-wizard state. \
                   `setup_completed` is derived from `wizard_step == \"completed\"`. \
                   No authentication required — the web UI calls this before \
                   rendering to decide whether to redirect to the wizard.",
    responses(
        (status = 200, description = "Current wizard state", body = SetupStatusResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn setup_status(
    State(state): State<AppState>,
) -> Result<Json<SetupStatusResponse>, AppError> {
    let state = state.auth_service().wizard_state().await?;
    Ok(Json(SetupStatusResponse {
        setup_completed: state.setup_completed(),
        wizard_step: state.step,
        wizard_mode: state.mode,
    }))
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

#[utoipa::path(
    post,
    path = "/api/setup/advance",
    tag = "setup",
    description = "Advance the setup wizard to a new step. Admin-authenticated — \
                   the web UI auto-logs the operator in immediately after \
                   `POST /api/setup` and uses normal admin auth from then on. \
                   Step transitions only move forward; \
                   `wizard_mode` is recorded when the operator picks the DHCP \
                   onboarding branch at step 3.",
    request_body = AdvanceWizardRequest,
    responses(
        (status = 200, description = "Wizard state after advance", body = AdvanceWizardResponse),
        (status = 400, description = "Invalid transition", body = ApiError),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn setup_advance(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<AdvanceWizardRequest>,
) -> Result<Json<AdvanceWizardResponse>, AppError> {
    let new_state = state
        .auth_service()
        .advance_wizard(body.to_step, body.wizard_mode)
        .await?;

    Ok(Json(AdvanceWizardResponse {
        wizard_step: new_state.step,
        wizard_mode: new_state.mode,
    }))
}
