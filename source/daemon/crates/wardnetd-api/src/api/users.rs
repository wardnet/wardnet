use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use wardnet_common::api::{ApiError, MeResponse};

use crate::api::middleware::AdminAuth;
use crate::api::responses::AuthErrors;
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register user-identity routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(me))
}

#[utoipa::path(
    get,
    path = "/api/users/me",
    tag = "users",
    description = "Return the authenticated admin's identity. Used by the \
                   web UI (e.g. the setup wizard's review step) to display \
                   the account name without a separate credential store.",
    responses(
        (status = 200, description = "Authenticated admin identity", body = MeResponse),
        AuthErrors,
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn me(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<MeResponse>, AppError> {
    let username = state.auth_service().current_admin_username().await?;
    Ok(Json(MeResponse { username }))
}
