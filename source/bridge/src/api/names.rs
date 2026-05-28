use axum::Json;
use axum::extract::{Path, State};
use serde::Serialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::api::validation::is_valid_name;
use crate::error::ApiError;
use crate::state::AppState;

/// Register the name-availability route.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(name_available))
}

/// Response body for `GET /v1/names/{name}/available`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct NameAvailabilityResponse {
    /// `true` if the name is not yet registered and passes all validation rules.
    pub available: bool,
}

#[utoipa::path(
    get,
    path = "/v1/names/{name}/available",
    tag = "installs",
    description = "Check whether a subdomain name is available for registration. \
                   Returns `available: false` for names that are already taken, \
                   reserved, or syntactically invalid. \
                   \n\n\
                   The daemon calls this endpoint during the setup wizard to give \
                   the user real-time feedback. No authentication required.",
    params(
        ("name" = String, Path, description = "Subdomain slug to check, e.g. `happy-einstein`"),
    ),
    responses(
        (status = 200, description = "Availability result", body = NameAvailabilityResponse),
        (status = 500, description = "Internal server error"),
    ),
    security(()),
)]
pub async fn name_available(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<NameAvailabilityResponse>, ApiError> {
    // Syntactically invalid or reserved names are immediately unavailable —
    // no DB round-trip needed.
    if !is_valid_name(&name) {
        return Ok(Json(NameAvailabilityResponse { available: false }));
    }

    let taken = state
        .installs()
        .find_by_name(&name)
        .await
        .map_err(ApiError::Internal)?
        .is_some();

    Ok(Json(NameAvailabilityResponse { available: !taken }))
}

#[cfg(test)]
mod tests;
