use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use wardnet_common::api::{ApiError, MeResponse};

use crate::api::middleware::SessionAuth;
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
    description = "Return the authenticated household user's identity. Used by \
                   the web UI (e.g. the setup wizard's review step) to display \
                   the account name without a separate credential store, and to \
                   decide which admin-only surfaces to render. Available to any \
                   authenticated user, including members reading their own \
                   profile.",
    responses(
        (status = 200, description = "Authenticated user identity", body = MeResponse),
        AuthErrors,
        (status = 500, description = "Internal server error", body = ApiError),
    ),
)]
pub async fn me(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<MeResponse>, AppError> {
    let user = state.auth_service().current_user().await?;
    Ok(Json(MeResponse {
        // `username` and `display_name` intentionally carry the same value: the
        // former is the pre-ADR-0031 field name that existing clients read, kept
        // so this stays an additive change.
        username: user.display_name.clone(),
        id: user.user_id.to_string(),
        display_name: user.display_name,
        email: user.email,
        role: user.role,
    }))
}
