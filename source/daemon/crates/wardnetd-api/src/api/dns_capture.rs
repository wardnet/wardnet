use axum::Json;
use axum::extract::{Path, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{
    ApiError, DeviceCaptureToggleRequest, DnsCaptureSettingsRequest, DnsCaptureSettingsResponse,
};

use crate::api::middleware::{ClientIp, SessionAuth};
use crate::state::AppState;
use wardnetd_services::error::AppError;

const TAG: &str = "devices";
const PATH: &str = "/api/devices/{id}/dns-capture";
const PATH_ME: &str = "/api/devices/me/dns-capture";

/// Register DNS capture routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(
            get_dns_capture_settings,
            update_dns_capture_settings
        ))
        .routes(routes!(set_my_dns_capture))
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
)]
pub async fn get_dns_capture_settings(
    State(state): State<AppState>,
    _auth: SessionAuth,
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
)]
pub async fn update_dns_capture_settings(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
    Json(body): Json<DnsCaptureSettingsRequest>,
) -> Result<Json<DnsCaptureSettingsResponse>, AppError> {
    // Omitted fields are merged atomically in the SQL UPDATE (COALESCE).
    state
        .device_service()
        .update_dns_capture_settings(&id, body.enabled, body.cap_count, body.cap_days)
        .await?;

    let response = state.device_service().get_dns_capture_settings(&id).await?;
    Ok(Json(response))
}

#[utoipa::path(
    patch,
    path = PATH_ME,
    tag = TAG,
    description = "Let the caller enable or disable DNS capture for their own device. \
                   The device is identified by source IP — no authentication is required \
                   (self-service by IP). Only the `enabled` flag is changed; retention \
                   caps (cap_count / cap_days) are admin-only and left untouched.",
    request_body = DeviceCaptureToggleRequest,
    responses(
        (status = 200, description = "Updated capture settings and stats", body = DnsCaptureSettingsResponse),
        (status = 400, description = "Malformed request body", body = ApiError),
        (status = 404, description = "Device not found for this IP", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn set_my_dns_capture(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<DeviceCaptureToggleRequest>,
) -> Result<Json<DnsCaptureSettingsResponse>, AppError> {
    let response = state
        .device_service()
        .set_my_capture_enabled(&ip.to_string(), body.enabled)
        .await?;
    Ok(Json(response))
}
