use axum::Json;
use axum::extract::State;
use wardnet_types::api::{DeviceMeResponse, SetMyRuleRequest, SetMyRuleResponse};

use crate::api::middleware::ClientIp;
use crate::error::AppError;
use crate::state::AppState;

/// GET /api/devices/me
///
/// Thin handler — identifies the caller by source IP and returns their
/// device info and current routing rule. No authentication required.
pub async fn get_me(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
) -> Result<Json<DeviceMeResponse>, AppError> {
    let response = state
        .device_service()
        .get_device_for_ip(&ip.to_string())
        .await?;
    Ok(Json(response))
}

/// PUT /api/devices/me/rule
///
/// Thin handler — allows the caller to set their own routing rule.
/// Delegates admin-lock checks to [`DeviceService`](crate::service::DeviceService).
/// No authentication required (self-service by IP).
pub async fn set_my_rule(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<SetMyRuleRequest>,
) -> Result<Json<SetMyRuleResponse>, AppError> {
    let response = state
        .device_service()
        .set_rule_for_ip(&ip.to_string(), body.target)
        .await?;
    Ok(Json(response))
}
