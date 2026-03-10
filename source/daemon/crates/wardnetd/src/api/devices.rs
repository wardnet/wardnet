use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;
use wardnet_types::api::{
    DeviceDetailResponse, DeviceMeResponse, ListDevicesResponse, SetMyRuleRequest,
    SetMyRuleResponse, UpdateDeviceRequest,
};

use crate::api::middleware::{AdminAuth, ClientIp};
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

/// GET /api/devices — List all devices (admin only).
pub async fn list_devices(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListDevicesResponse>, AppError> {
    let devices = state.discovery_service().get_all_devices().await?;
    Ok(Json(ListDevicesResponse { devices }))
}

/// GET /api/devices/:id — Get device detail with routing rule (admin only).
pub async fn get_device(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
) -> Result<Json<DeviceDetailResponse>, AppError> {
    let uuid: Uuid = id
        .parse()
        .map_err(|_| AppError::BadRequest("invalid device ID".to_owned()))?;
    let device = state.discovery_service().get_device_by_id(uuid).await?;
    let rule = state
        .device_service()
        .get_device_for_ip(&device.last_ip)
        .await
        .ok()
        .and_then(|r| r.current_rule);
    Ok(Json(DeviceDetailResponse {
        device,
        current_rule: rule,
    }))
}

/// PUT /api/devices/:id — Update device properties (admin only).
///
/// Supports updating name, device type, routing target, and admin-locked flag.
/// Each field is optional; only provided fields are changed.
pub async fn update_device(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
    Json(body): Json<UpdateDeviceRequest>,
) -> Result<Json<DeviceDetailResponse>, AppError> {
    let uuid: Uuid = id
        .parse()
        .map_err(|_| AppError::BadRequest("invalid device ID".to_owned()))?;

    // Update name and type if provided.
    let device = state
        .discovery_service()
        .update_device(uuid, body.name.as_deref(), body.device_type)
        .await?;

    let device_id_str = device.id.to_string();

    // Update admin_locked if provided.
    if let Some(locked) = body.admin_locked {
        state
            .device_service()
            .update_admin_locked(&device_id_str, locked)
            .await?;
    }

    // Update routing rule if provided.
    if let Some(target) = body.routing_target {
        state
            .device_service()
            .set_rule(&device_id_str, target)
            .await?;
    }

    // Fetch current rule for the response.
    let rule = state
        .device_service()
        .get_device_for_ip(&device.last_ip)
        .await
        .ok()
        .and_then(|r| r.current_rule);

    Ok(Json(DeviceDetailResponse {
        device,
        current_rule: rule,
    }))
}
