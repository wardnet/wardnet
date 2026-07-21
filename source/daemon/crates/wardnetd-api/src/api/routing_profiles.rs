//! Routing-profile HTTP handlers — `/api/routing/...` (issue #241).
//!
//! CRUD over routing profiles and their domain rules, plus the ordered
//! device → profile assignment. Parallel to [`super::dns_filter`]; the
//! per-device routing binding and default policy stay in the routing endpoints
//! under [`super::devices`] / [`super::system`].

use axum::Json;
use axum::extract::{Path, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use wardnet_common::api::{
    CreateDomainRoutingRuleRequest, CreateDomainRoutingRuleResponse, CreateRoutingProfileRequest,
    CreateRoutingProfileResponse, DeleteDomainRoutingRuleResponse, DeleteRoutingProfileResponse,
    GetDeviceRoutingProfilesResponse, GetRoutingProfileResponse, ListDomainRoutingRulesResponse,
    ListProfileDevicesResponse, ListRoutingProfilesResponse, SetDeviceRoutingProfilesRequest,
    SetDeviceRoutingProfilesResponse, UpdateDomainRoutingRuleRequest, UpdateDomainRoutingRuleResponse,
    UpdateRoutingProfileRequest, UpdateRoutingProfileResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest, Conflict, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_profiles, create_profile))
        .routes(routes!(get_profile, update_profile, delete_profile))
        .routes(routes!(list_rules, create_rule))
        .routes(routes!(update_rule, delete_rule))
        .routes(routes!(get_device_profiles, set_device_profiles))
        .routes(routes!(get_profile_devices))
}

// ── Profiles ────────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/routing/profiles",
    tag = "routing",
    description = "List every routing profile. Admin only.",
    responses(
        (status = 200, description = "Routing profiles", body = ListRoutingProfilesResponse),
        AuthErrors,
    ),
)]
pub async fn list_profiles(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListRoutingProfilesResponse>, AppError> {
    let profiles = state.routing_profile_service().list_profiles().await?;
    Ok(Json(ListRoutingProfilesResponse { profiles }))
}

#[utoipa::path(
    post,
    path = "/api/routing/profiles",
    tag = "routing",
    description = "Create a routing profile. Admin only.",
    request_body = CreateRoutingProfileRequest,
    responses(
        (status = 201, description = "Profile created", body = CreateRoutingProfileResponse),
        AuthErrors,
        BadRequest,
        Conflict,
    ),
)]
pub async fn create_profile(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(req): Json<CreateRoutingProfileRequest>,
) -> Result<(axum::http::StatusCode, Json<CreateRoutingProfileResponse>), AppError> {
    let profile = state
        .routing_profile_service()
        .create_profile(&req.name)
        .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateRoutingProfileResponse {
            profile,
            message: "Routing profile created".to_owned(),
        }),
    ))
}

#[utoipa::path(
    get,
    path = "/api/routing/profiles/{id}",
    tag = "routing",
    description = "Get a single routing profile. Admin only.",
    params(("id" = Uuid, Path, description = "Profile id")),
    responses(
        (status = 200, description = "Routing profile", body = GetRoutingProfileResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn get_profile(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<GetRoutingProfileResponse>, AppError> {
    let profile = state.routing_profile_service().get_profile(id).await?;
    Ok(Json(GetRoutingProfileResponse { profile }))
}

#[utoipa::path(
    put,
    path = "/api/routing/profiles/{id}",
    tag = "routing",
    description = "Rename a routing profile. Admin only.",
    params(("id" = Uuid, Path, description = "Profile id")),
    request_body = UpdateRoutingProfileRequest,
    responses(
        (status = 200, description = "Profile updated", body = UpdateRoutingProfileResponse),
        AuthErrors,
        BadRequest,
        NotFound,
        Conflict,
    ),
)]
pub async fn update_profile(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateRoutingProfileRequest>,
) -> Result<Json<UpdateRoutingProfileResponse>, AppError> {
    let Some(name) = req.name else {
        return Err(AppError::BadRequest("nothing to update".to_owned()));
    };
    let profile = state
        .routing_profile_service()
        .rename_profile(id, &name)
        .await?;
    Ok(Json(UpdateRoutingProfileResponse {
        profile,
        message: "Routing profile updated".to_owned(),
    }))
}

#[utoipa::path(
    delete,
    path = "/api/routing/profiles/{id}",
    tag = "routing",
    description = "Delete a routing profile and its rules and assignments. Admin only.",
    params(("id" = Uuid, Path, description = "Profile id")),
    responses(
        (status = 200, description = "Profile deleted", body = DeleteRoutingProfileResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn delete_profile(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteRoutingProfileResponse>, AppError> {
    state.routing_profile_service().delete_profile(id).await?;
    Ok(Json(DeleteRoutingProfileResponse {
        message: "Routing profile deleted".to_owned(),
    }))
}

// ── Rules ─────────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/routing/profiles/{id}/rules",
    tag = "routing",
    description = "List the domain rules in a routing profile. Admin only.",
    params(("id" = Uuid, Path, description = "Profile id")),
    responses(
        (status = 200, description = "Domain routing rules", body = ListDomainRoutingRulesResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn list_rules(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<ListDomainRoutingRulesResponse>, AppError> {
    let rules = state.routing_profile_service().list_rules(id).await?;
    Ok(Json(ListDomainRoutingRulesResponse { rules }))
}

#[utoipa::path(
    post,
    path = "/api/routing/profiles/{id}/rules",
    tag = "routing",
    description = "Add a domain rule to a routing profile. Admin only.",
    params(("id" = Uuid, Path, description = "Profile id")),
    request_body = CreateDomainRoutingRuleRequest,
    responses(
        (status = 201, description = "Rule created", body = CreateDomainRoutingRuleResponse),
        AuthErrors,
        BadRequest,
        NotFound,
        Conflict,
    ),
)]
pub async fn create_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(req): Json<CreateDomainRoutingRuleRequest>,
) -> Result<
    (
        axum::http::StatusCode,
        Json<CreateDomainRoutingRuleResponse>,
    ),
    AppError,
> {
    let rule = state
        .routing_profile_service()
        .create_rule(id, &req.pattern, req.target, req.enabled)
        .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateDomainRoutingRuleResponse {
            rule,
            message: "Routing rule created".to_owned(),
        }),
    ))
}

#[utoipa::path(
    put,
    path = "/api/routing/rules/{id}",
    tag = "routing",
    description = "Update a domain routing rule. Admin only.",
    params(("id" = Uuid, Path, description = "Rule id")),
    request_body = UpdateDomainRoutingRuleRequest,
    responses(
        (status = 200, description = "Rule updated", body = UpdateDomainRoutingRuleResponse),
        AuthErrors,
        BadRequest,
        NotFound,
        Conflict,
    ),
)]
pub async fn update_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateDomainRoutingRuleRequest>,
) -> Result<Json<UpdateDomainRoutingRuleResponse>, AppError> {
    let rule = state
        .routing_profile_service()
        .update_rule(id, req.pattern, req.target, req.enabled)
        .await?;
    Ok(Json(UpdateDomainRoutingRuleResponse {
        rule,
        message: "Routing rule updated".to_owned(),
    }))
}

#[utoipa::path(
    delete,
    path = "/api/routing/rules/{id}",
    tag = "routing",
    description = "Delete a domain routing rule. Admin only.",
    params(("id" = Uuid, Path, description = "Rule id")),
    responses(
        (status = 200, description = "Rule deleted", body = DeleteDomainRoutingRuleResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn delete_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteDomainRoutingRuleResponse>, AppError> {
    state.routing_profile_service().delete_rule(id).await?;
    Ok(Json(DeleteDomainRoutingRuleResponse {
        message: "Routing rule deleted".to_owned(),
    }))
}

// ── Device assignment (ordered) ──────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/routing/devices/{device_id}/profiles",
    tag = "routing",
    description = "List a device's assigned routing profiles, in priority order. Admin only.",
    params(("device_id" = Uuid, Path, description = "Device id")),
    responses(
        (status = 200, description = "Assigned profiles", body = GetDeviceRoutingProfilesResponse),
        AuthErrors,
    ),
)]
pub async fn get_device_profiles(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(device_id): Path<Uuid>,
) -> Result<Json<GetDeviceRoutingProfilesResponse>, AppError> {
    let profile_ids = state
        .routing_profile_service()
        .get_device_profiles(device_id)
        .await?;
    Ok(Json(GetDeviceRoutingProfilesResponse { profile_ids }))
}

#[utoipa::path(
    put,
    path = "/api/routing/devices/{device_id}/profiles",
    tag = "routing",
    description = "Set a device's routing profiles in priority order (first = highest \
                   priority). Replaces the whole assignment. Admin only.",
    params(("device_id" = Uuid, Path, description = "Device id")),
    request_body = SetDeviceRoutingProfilesRequest,
    responses(
        (status = 200, description = "Assignment updated", body = SetDeviceRoutingProfilesResponse),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn set_device_profiles(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(device_id): Path<Uuid>,
    Json(req): Json<SetDeviceRoutingProfilesRequest>,
) -> Result<Json<SetDeviceRoutingProfilesResponse>, AppError> {
    state
        .routing_profile_service()
        .set_device_profiles(device_id, &req.profile_ids)
        .await?;
    Ok(Json(SetDeviceRoutingProfilesResponse {
        message: "Device routing profiles updated".to_owned(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/routing/profiles/{id}/devices",
    tag = "routing",
    description = "List the devices a routing profile is assigned to. Admin only.",
    params(("id" = Uuid, Path, description = "Profile id")),
    responses(
        (status = 200, description = "Assigned devices", body = ListProfileDevicesResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn get_profile_devices(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<ListProfileDevicesResponse>, AppError> {
    let device_ids = state
        .routing_profile_service()
        .list_profile_devices(id)
        .await?;
    Ok(Json(ListProfileDevicesResponse { device_ids }))
}
