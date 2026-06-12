use axum::Json;
use axum::extract::{Path, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{ApiError, DnsCaptureSettingsRequest, DnsCaptureSettingsResponse};

use crate::api::middleware::AdminAuth;
use crate::state::AppState;
use wardnetd_services::error::AppError;

const TAG: &str = "devices";
const PATH: &str = "/api/devices/{id}/dns-capture";

/// Register DNS capture routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(
        get_dns_capture_settings,
        update_dns_capture_settings
    ))
}

#[utoipa::path(
    get,
    path = PATH,
    tag = TAG,
    description = "Return current DNS capture settings and storage stats for a device.",
    params(
        ("id" = String, Path, description = "Device UUID"),
    ),
    responses(
        (status = 200, description = "Current capture settings and stats", body = DnsCaptureSettingsResponse),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 404, description = "Device not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(("admin_auth" = [])),
)]
pub async fn get_dns_capture_settings(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
) -> Result<Json<DnsCaptureSettingsResponse>, AppError> {
    let response = state.device_service().get_dns_capture_settings(&id).await?;
    Ok(Json(response))
}

#[utoipa::path(
    patch,
    path = PATH,
    tag = TAG,
    description = "Update DNS capture settings for a device. Omitted fields are left unchanged.",
    params(
        ("id" = String, Path, description = "Device UUID"),
    ),
    request_body = DnsCaptureSettingsRequest,
    responses(
        (status = 200, description = "Updated capture settings and stats", body = DnsCaptureSettingsResponse),
        (status = 400, description = "Invalid request body", body = ApiError),
        (status = 401, description = "Authentication required", body = ApiError),
        (status = 404, description = "Device not found", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(("admin_auth" = [])),
)]
pub async fn update_dns_capture_settings(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
    Json(body): Json<DnsCaptureSettingsRequest>,
) -> Result<Json<DnsCaptureSettingsResponse>, AppError> {
    // Load current settings to merge with any omitted fields.
    let current = state.device_service().get_dns_capture_settings(&id).await?;

    let enabled = body.enabled.unwrap_or(current.enabled);
    let cap_count = body.cap_count.unwrap_or(current.cap_count);
    let cap_days = body.cap_days.unwrap_or(current.cap_days);

    state
        .device_service()
        .update_dns_capture_settings(&id, enabled, cap_count, cap_days)
        .await?;

    let response = state.device_service().get_dns_capture_settings(&id).await?;
    Ok(Json(response))
}
