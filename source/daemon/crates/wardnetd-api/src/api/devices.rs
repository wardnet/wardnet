use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    ApiError, DeviceDetailResponse, DeviceMeResponse, DeviceWithStatus, ListDevicesResponse,
    SetMyRuleRequest, SetMyRuleResponse, UpdateDeviceRequest,
};
use wardnet_common::device::DhcpStatus;

use crate::api::middleware::{AdminAuth, ClientIp};
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register device routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_devices))
        .routes(routes!(get_me))
        .routes(routes!(set_my_rule))
        .routes(routes!(get_device, update_device))
}

const TAG: &str = "devices";
const PATH_ME: &str = "/api/devices/me";
const PATH_ME_RULE: &str = "/api/devices/me/rule";
const PATH_LIST: &str = "/api/devices";
const PATH_ITEM: &str = "/api/devices/{id}";

#[utoipa::path(
    get,
    path = PATH_ME,
    tag = TAG,
    description = "Identify the caller by source IP and return their device info, \
                   current routing rule, and the list of available tunnels they can \
                   route through. Used by the self-service \"My Device\" page, so no \
                   authentication is required — the daemon trusts the source IP of the \
                   incoming TCP connection.",
    responses(
        (status = 200, description = "Caller device info and available tunnels", body = DeviceMeResponse),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn get_me(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
) -> Result<Json<DeviceMeResponse>, AppError> {
    let mut response = state
        .device_service()
        .get_device_for_ip(&ip.to_string())
        .await?;

    // Enrich with available tunnels for self-service routing selection.
    // Uses an internal admin context since tunnel listing is admin-only.
    let tunnels = wardnetd_services::auth_context::with_context(
        wardnet_common::auth::AuthContext::Admin {
            admin_id: uuid::Uuid::nil(),
        },
        state.tunnel_service().list_tunnels(),
    )
    .await
    .map(|r| {
        r.tunnels
            .into_iter()
            .map(|t| wardnet_common::api::TunnelSummary {
                id: t.id.to_string(),
                label: t.label,
                country_code: t.country_code,
            })
            .collect()
    })
    .unwrap_or_default();
    response.available_tunnels = tunnels;

    Ok(Json(response))
}

#[utoipa::path(
    put,
    path = PATH_ME_RULE,
    tag = TAG,
    description = "Let the caller change their own routing rule (direct vs. a specific \
                   tunnel). The target device is identified by source IP. Admin-locked \
                   devices are rejected with 403 — an admin must lift the lock first. \
                   No authentication is required (self-service by IP).",
    request_body = SetMyRuleRequest,
    responses(
        (status = 200, description = "Updated caller routing rule", body = SetMyRuleResponse),
        (status = 400, description = "Malformed request body", body = ApiError),
        (status = 403, description = "Device is admin-locked", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
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

/// Build a lookup table from MAC address to [`DhcpStatus`].
///
/// Reservations take precedence: if a MAC has both a reservation and an
/// active lease the status is [`DhcpStatus::Reservation`].
async fn build_dhcp_status_map(state: &AppState) -> Result<HashMap<String, DhcpStatus>, AppError> {
    let leases = state.dhcp_service().list_leases().await?;
    let reservations = state.dhcp_service().list_reservations().await?;

    let mut map = HashMap::new();

    for lease in &leases.leases {
        map.insert(lease.mac_address.to_lowercase(), DhcpStatus::Lease);
    }

    // Reservations override leases.
    for res in &reservations.reservations {
        map.insert(res.mac_address.to_lowercase(), DhcpStatus::Reservation);
    }

    Ok(map)
}

/// Enrich a [`Device`](wardnet_common::device::Device) with its DHCP status.
fn enrich_device(
    device: wardnet_common::device::Device,
    dhcp_map: &HashMap<String, DhcpStatus>,
) -> DeviceWithStatus {
    let status = dhcp_map
        .get(&device.mac.to_lowercase())
        .copied()
        .unwrap_or(DhcpStatus::External);
    DeviceWithStatus {
        device,
        dhcp_status: status,
    }
}

#[utoipa::path(
    get,
    path = PATH_LIST,
    tag = TAG,
    description = "List every device the daemon has seen on the LAN, enriched with its \
                   current DHCP status (reservation, active lease, or external/unknown). \
                   Admin only.",
    responses(
        (status = 200, description = "List of devices with DHCP status", body = ListDevicesResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_devices(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListDevicesResponse>, AppError> {
    let devices = state.discovery_service().get_all_devices().await?;
    let dhcp_map = build_dhcp_status_map(&state).await?;

    let devices = devices
        .into_iter()
        .map(|d| enrich_device(d, &dhcp_map))
        .collect();

    Ok(Json(ListDevicesResponse { devices }))
}

#[utoipa::path(
    get,
    path = PATH_ITEM,
    tag = TAG,
    description = "Fetch a single device by ID, including its current routing rule and \
                   DHCP status. Admin only.",
    params(("id" = Uuid, Path, description = "Device ID")),
    responses(
        (status = 200, description = "Device detail", body = DeviceDetailResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
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
    let dhcp_map = build_dhcp_status_map(&state).await?;
    let device = enrich_device(device, &dhcp_map);
    Ok(Json(DeviceDetailResponse {
        device,
        current_rule: rule,
    }))
}

#[utoipa::path(
    put,
    path = PATH_ITEM,
    tag = TAG,
    description = "Update mutable device properties: display name, device type, routing \
                   target, and the admin-locked flag. Each field is optional; only \
                   fields present in the request body are changed. Admin only.",
    params(("id" = Uuid, Path, description = "Device ID")),
    request_body = UpdateDeviceRequest,
    responses(
        (status = 200, description = "Updated device detail", body = DeviceDetailResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
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

    let dhcp_map = build_dhcp_status_map(&state).await?;
    let device = enrich_device(device, &dhcp_map);
    Ok(Json(DeviceDetailResponse {
        device,
        current_rule: rule,
    }))
}
