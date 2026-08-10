use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    ApiError, AssignDeviceZoneRequest, DeviceDetailResponse, DeviceMeResponse, DeviceWithStatus,
    ListDevicesResponse, RoutingProfileSummary, SetMyRuleRequest, SetMyRuleResponse,
    UpdateDeviceRequest, ZoneSummary,
};
use wardnet_common::device::DhcpStatus;
use wardnet_common::routing::RoutingTarget;

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
        .routes(routes!(assign_device_zone))
        .routes(routes!(release_device))
}

const TAG: &str = "devices";
const PATH_ME: &str = "/api/devices/me";
const PATH_ME_RULE: &str = "/api/devices/me/rule";
const PATH_LIST: &str = "/api/devices";
const PATH_ITEM: &str = "/api/devices/{id}";

#[utoipa::path(
    operation_id = "devices_get_me",
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
                status: t.status,
                last_handshake: t.last_handshake,
            })
            .collect()
    })
    .unwrap_or_default();
    response.available_tunnels = tunnels;

    // Enrich with the caller's own zone (name + is_default) for the read-only
    // zone display in the user PWA. The zone catalog is admin-only, so — like
    // the tunnel listing above — we resolve it under an internal admin context.
    if let Some(device) = response.device.as_ref() {
        let zone_id = device.zone_id;
        let device_id = device.id;
        response.zone = wardnetd_services::auth_context::with_context(
            wardnet_common::auth::AuthContext::Admin {
                admin_id: uuid::Uuid::nil(),
            },
            state.network_zone_service().get_zone(zone_id),
        )
        .await
        .ok()
        .map(|z| ZoneSummary {
            name: z.zone.name,
            is_default: z.zone.is_default,
        });

        // Enrich with the caller's assigned routing profiles (id + name, in
        // priority order) for the read-only "profiles applied to your device"
        // display. Profile management is admin-only, so — like the tunnel and
        // zone lookups above — resolve under an internal admin context.
        response.routing_profiles = wardnetd_services::auth_context::with_context(
            wardnet_common::auth::AuthContext::Admin {
                admin_id: uuid::Uuid::nil(),
            },
            async {
                let svc = state.routing_profile_service();
                let ids = svc.get_device_profiles(device_id).await?;
                // Resolve each assigned profile by id — the cost is bounded by
                // the device's assignment count (an indexed primary-key lookup
                // each) rather than scanning the whole profile catalogue, which
                // matters on this 10s-polled `/me` endpoint.
                let mut summaries = Vec::with_capacity(ids.len());
                for id in ids {
                    // A profile can be deleted between the assignment read and
                    // this lookup; skip it rather than failing the /me response.
                    if let Ok(profile) = svc.get_profile(id).await {
                        summaries.push(RoutingProfileSummary {
                            id: profile.id.to_string(),
                            name: profile.name,
                        });
                    }
                }
                Ok::<_, AppError>(summaries)
            },
        )
        .await
        .unwrap_or_default();
    }

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
///
/// MAC casing is canonical (lowercase) across the data layer (issue #312),
/// so map keys and lookups don't need per-call normalisation.
async fn build_dhcp_status_map(state: &AppState) -> Result<HashMap<String, DhcpStatus>, AppError> {
    let leases = state.dhcp_service().list_leases().await?;
    let reservations = state.dhcp_service().list_reservations().await?;

    let mut map = HashMap::new();

    for lease in &leases.leases {
        map.insert(lease.mac_address.clone(), DhcpStatus::Lease);
    }

    // Reservations override leases.
    for res in &reservations.reservations {
        map.insert(res.mac_address.clone(), DhcpStatus::Reservation);
    }

    Ok(map)
}

/// Build the `DeviceDetailResponse` the three detail-returning handlers share.
///
/// `signals_are_fatal` splits the read path from the two mutation paths, and
/// the split is the whole point. An empty `signals` list is not "unknown" — the
/// UI renders it as the positive claim "nothing observed yet". On `GET` we
/// therefore refuse to guess: a failed read surfaces as an error rather than as
/// a confident falsehood. `PUT` has already committed its write by this point,
/// so failing there would report an error for a mutation that succeeded; it
/// degrades to an empty list and logs, and the client's next `GET` tells the
/// truth.
async fn device_detail(
    state: &AppState,
    device: wardnet_common::device::Device,
    signals_are_fatal: bool,
) -> Result<DeviceDetailResponse, AppError> {
    let device_id = device.id.to_string();
    let rule = state
        .device_service()
        .get_rule_for_device(&device_id)
        .await
        .ok()
        .flatten();

    let signals = match state
        .device_identification_service()
        .signals_for(&device_id)
        .await
    {
        Ok(signals) => signals,
        Err(err) if signals_are_fatal => return Err(err),
        Err(err) => {
            tracing::warn!(
                %device_id,
                error = %err,
                "failed to read identification signals for device_id={device_id}: {err}"
            );
            Vec::new()
        }
    };

    let dhcp_map = build_dhcp_status_map(state).await?;
    Ok(DeviceDetailResponse {
        device: enrich_device(device, &dhcp_map, rule.clone()),
        current_rule: rule,
        signals,
    })
}

/// Enrich a [`Device`](wardnet_common::device::Device) with its DHCP status
/// and current routing target. `current_rule` is `None` when the device has no
/// rule of its own (it follows the gateway default policy).
fn enrich_device(
    device: wardnet_common::device::Device,
    dhcp_map: &HashMap<String, DhcpStatus>,
    current_rule: Option<RoutingTarget>,
) -> DeviceWithStatus {
    let status = dhcp_map
        .get(&device.mac)
        .copied()
        .unwrap_or(DhcpStatus::External);
    DeviceWithStatus {
        device,
        dhcp_status: status,
        current_rule,
    }
}

#[utoipa::path(
    get,
    path = PATH_LIST,
    tag = TAG,
    description = "List every device the daemon has seen on the LAN, enriched with its \
                   current DHCP status (reservation, active lease, or external/unknown) \
                   and current routing target (a specific tunnel, direct, or null when \
                   the device follows the gateway default policy). Admin only.",
    responses(
        (status = 200, description = "List of devices with DHCP status", body = ListDevicesResponse),
        AuthErrors,
    ),
)]
pub async fn list_devices(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListDevicesResponse>, AppError> {
    let devices = state.discovery_service().get_all_devices().await?;
    let dhcp_map = build_dhcp_status_map(&state).await?;
    // Single batched lookup of every device's routing rule — no N+1.
    let mut routing_map = state.device_service().current_rules().await?;

    let devices = devices
        .into_iter()
        .map(|d| {
            let rule = routing_map.remove(&d.id);
            enrich_device(d, &dhcp_map, rule)
        })
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
    // Read path: a failed signals read is an error, not an empty list.
    Ok(Json(device_detail(&state, device, true).await?))
}

#[utoipa::path(
    put,
    path = PATH_ITEM,
    tag = TAG,
    description = "Update mutable device properties: display name, device type, routing \
                   target, and the admin-locked flag. Each field is optional; only \
                   fields present in the request body are changed. Partial updates \
                   are best-effort — the mutations are not transactional across \
                   services, but they are ordered most-likely-to-fail first so a \
                   rejected routing rule leaves the name and admin-locked flag \
                   unchanged. Admin only.",
    params(("id" = Uuid, Path, description = "Device ID")),
    request_body = UpdateDeviceRequest,
    responses(
        (status = 200, description = "Updated device detail", body = DeviceDetailResponse),
        AuthErrors,
        NotFound,
        BadRequest,
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

    // Order deliberately runs the mutations most likely to reject first so
    // a failure happens before any later field is touched: routing-rule
    // validation (tunnel exists, target shape) is the usual failure mode,
    // admin_locked is a trivial bool flip, and the name/type update is
    // idempotent. This is still a best-effort partial write — the three
    // mutations are NOT transactional across services — but the reorder
    // removes the surface where "rule rejected" leaves an unrelated name
    // change persisted. Partial-update semantics are documented in the
    // `#[utoipa::path(description = …)]` block above.
    let device_id_str = uuid.to_string();

    if let Some(target) = body.routing_target {
        state
            .device_service()
            .set_rule(&device_id_str, target)
            .await?;
    }

    if let Some(locked) = body.admin_locked {
        state
            .device_service()
            .update_admin_locked(&device_id_str, locked)
            .await?;
    }

    let device = state
        .discovery_service()
        .update_device(uuid, body.name.as_deref(), body.device_type)
        .await?;

    // Mutation path: the write above already landed, so a failed signals read
    // degrades rather than reporting an error for a change that succeeded.
    Ok(Json(device_detail(&state, device, false).await?))
}

#[utoipa::path(
    put,
    path = "/api/devices/{id}/zone",
    tag = TAG,
    description = "Reassign a device to a different Network Zone (epic #244, \
                   issue #735). Membership is otherwise sticky (set once at \
                   discovery). Returns the updated device detail. Admin only.",
    params(("id" = Uuid, Path, description = "Device ID")),
    request_body = AssignDeviceZoneRequest,
    responses(
        (status = 200, description = "Updated device detail", body = DeviceDetailResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn assign_device_zone(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
    Json(body): Json<AssignDeviceZoneRequest>,
) -> Result<Json<DeviceDetailResponse>, AppError> {
    let uuid: Uuid = id
        .parse()
        .map_err(|_| AppError::BadRequest("invalid device ID".to_owned()))?;
    state
        .network_zone_service()
        .assign_device(uuid, body.zone_id)
        .await?;

    let device = state.discovery_service().get_device_by_id(uuid).await?;
    // Mutation path — see `device_detail`.
    Ok(Json(device_detail(&state, device, false).await?))
}

/// Run one step of the release sequence, tagging any failure with the step's
/// name so a 500 says which revert did not land.
///
/// The device is left **managed** on any failure — `clear_managed` is the last
/// step and never runs — so a partial release is always recoverable by
/// retrying, never a half-released device carrying live credentials it is no
/// longer recorded as owning.
fn releasing(step: &'static str, result: Result<(), AppError>) -> Result<(), AppError> {
    result.map_err(|e| match e {
        // A guard rejection is about the caller, not the step; keep its status.
        e @ (AppError::Unauthorized(_) | AppError::Forbidden(_)) => e,
        e => AppError::Internal(anyhow::anyhow!("release failed at step '{step}': {e}")),
    })
}

/// Revoke the release steps that live in collections which must be scanned for
/// the device: its Private-DNS grant, Remote peer credential, DHCP reservation,
/// and zone exceptions.
///
/// Split out of [`release_device`] for length only; it is the first half of
/// that function's ordered sequence and must not be reordered against it.
///
/// The Private-DNS grant goes first because revoking publishes
/// `PrivateDnsGrantRevoked`, which tears down the device's already-established
/// `DoT` sessions — the token is otherwise only checked at handshake, so a
/// revoked device would keep resolving until it happened to reconnect.
async fn release_credentials_and_scoped_rules(
    state: &AppState,
    device_id: Uuid,
    mac: &str,
) -> Result<(), AppError> {
    if let Some(grant) = state.private_dns_service().device_grant(device_id).await? {
        releasing(
            "revoke private-DNS grant",
            state.private_dns_service().revoke_grant(grant.id).await,
        )?;
    }

    let peers = state.inbound_wg_service().list_peers().await?;
    for peer in peers.iter().filter(|p| p.device_id == Some(device_id)) {
        releasing(
            "remove remote peer",
            state.inbound_wg_service().remove_peer(peer.id).await,
        )?;
    }

    // Reservations are keyed by MAC, not device id — the one artefact that
    // would otherwise survive the device row entirely.
    let reservations = state.dhcp_service().list_reservations().await?;
    for reservation in reservations
        .reservations
        .iter()
        .filter(|r| r.mac_address.eq_ignore_ascii_case(mac))
    {
        releasing(
            "delete DHCP reservation",
            state
                .dhcp_service()
                .delete_reservation(reservation.id)
                .await
                .map(|_| ()),
        )?;
    }

    // Exceptions naming this device on either side.
    let exceptions = state.zone_exception_service().list_exceptions().await?;
    for exception in exceptions.iter().filter(|e| {
        [&e.from, &e.to].iter().any(|endpoint| {
            endpoint.kind == wardnet_common::zone_exception::ExceptionEndpointKind::Device
                && endpoint.id == device_id
        })
    }) {
        releasing(
            "delete zone exception",
            state
                .zone_exception_service()
                .delete_exception(exception.id)
                .await,
        )?;
    }

    Ok(())
}

#[utoipa::path(
    post,
    path = "/api/devices/{id}/release",
    tag = TAG,
    description = "Stop managing a device (issue #1181). Reverts every \
                   admin-set configuration to default and returns the device to \
                   unmanaged, after which it becomes subject to device \
                   retention and is deleted once it has been absent for 30 \
                   days. Destructive: this revokes the device's Private-DNS \
                   grant and its Remote peer credential, disconnecting it. \
                   Idempotent — each step is a no-op when there is nothing to \
                   revert, so a retry after a partial failure completes. \
                   Admin only.",
    params(("id" = Uuid, Path, description = "Device ID")),
    responses(
        (status = 200, description = "Updated device detail", body = DeviceDetailResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn release_device(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
) -> Result<Json<DeviceDetailResponse>, AppError> {
    let uuid: Uuid = id
        .parse()
        .map_err(|_| AppError::BadRequest("invalid device ID".to_owned()))?;
    let device_id = uuid.to_string();

    // 404 up front rather than silently "releasing" a device that isn't there:
    // every step below is individually tolerant of a missing device, so without
    // this the whole sequence would succeed against nothing.
    let device = state.discovery_service().get_device_by_id(uuid).await?;

    // This orchestration lives in the handler, not in `DeviceService`, and that
    // is forced rather than stylistic: `InboundWgServiceImpl` and
    // `PrivateDnsServiceImpl` both hold `Arc<dyn DeviceService>` (they call
    // `mark_managed`), so a `DeviceService` that reached back into them would
    // be an `Arc` cycle and a construction-order deadlock. This handler already
    // orchestrates several services, so it is the established home.
    //
    // Order matters. `clear_managed` is LAST, so the invariant device retention
    // depends on — `managed = false` implies no admin artefacts exist — is only
    // asserted once every artefact is actually gone. Reversed, a failure
    // midway would leave an unmanaged device still holding a live Private-DNS
    // grant, which the prune would then delete 30 days later with nothing but a
    // log line to show for it.

    // 1-4. The artefacts held in collections that have to be scanned for this
    //      device. Split out only to keep this function readable — the ordering
    //      inside it is part of the same sequence.
    release_credentials_and_scoped_rules(&state, uuid, &device.mac).await?;

    // 5. Routing rule + routing-profile assignments. The rule is *deleted*, not
    //    set to `Direct`: "no rule" is the state a never-configured device is
    //    in (it follows the gateway's global default policy), whereas a
    //    `Direct` rule is an explicit choice that overrides that default — and
    //    is validated against the device's zone, so a tunnel-only zone would
    //    reject it and make such a device impossible to release.
    releasing(
        "clear routing rule",
        state.device_service().clear_rule(&device_id).await,
    )?;
    releasing(
        "clear routing profiles",
        state
            .routing_profile_service()
            .set_device_profiles(uuid, &[])
            .await,
    )?;

    // 6. DNS-filter settings and profile assignments, back to `default_for`:
    //    filtering enabled, no profiles.
    releasing(
        "reset DNS filter settings",
        state
            .dns_filter_service()
            .update_device_settings(
                uuid,
                wardnet_common::api::UpdateDeviceFilterSettingsRequest {
                    enabled: true,
                    profile_ids: Vec::new(),
                },
            )
            .await
            .map(|_| ()),
    )?;

    // 7. DNS capture off, admin lock cleared, name cleared.
    releasing(
        "disable DNS capture",
        state
            .device_service()
            .update_dns_capture_settings(&device_id, Some(false), None, None)
            .await,
    )?;
    releasing(
        "clear admin lock",
        state
            .device_service()
            .update_admin_locked(&device_id, false)
            .await,
    )?;
    releasing(
        "clear device name",
        state
            .discovery_service()
            .update_device(uuid, Some(""), None)
            .await
            .map(|_| ()),
    )?;

    // 8. Zone back to the current default-for-new — the zone a freshly
    //    discovered device would land in. Membership is sticky, so nothing else
    //    would ever move it back.
    let zones = state.network_zone_service().list_zones().await?;
    if let Some(default_zone) = zones.iter().find(|z| z.zone.is_default_for_new)
        && default_zone.zone.id != device.zone_id
    {
        releasing(
            "reset network zone",
            state
                .network_zone_service()
                .assign_device(uuid, default_zone.zone.id)
                .await,
        )?;
    }

    // 9. Finally unmanaged. Every step above that promotes on write (the
    //    routing rule, the filter settings, the zone) has already run, so this
    //    lands last and stays.
    releasing(
        "clear managed flag",
        state.device_service().clear_managed(&device_id).await,
    )?;

    tracing::info!(
        device_id = %uuid,
        mac = %device.mac,
        "released device {uuid} (mac {mac}): configuration reverted to default, now unmanaged",
        mac = device.mac,
    );

    let device = state.discovery_service().get_device_by_id(uuid).await?;
    // Mutation path — see `device_detail`.
    Ok(Json(device_detail(&state, device, false).await?))
}
