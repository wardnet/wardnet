//! DNS Filter HTTP handlers — `/api/dns/filter/...`. Issue #221.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    CreateAllowlistRequest, CreateAllowlistResponse, CreateBlocklistRequest,
    CreateBlocklistResponse, CreateFilterRuleRequest, CreateFilterRuleResponse,
    CreateProfileRequest, CreateProfileResponse, DeleteAllowlistResponse, DeleteBlocklistResponse,
    DeleteFilterRuleResponse, DeleteProfileResponse, DnsFilterConfigResponse,
    GetDeviceFilterSettingsResponse, GetProfileResponse, ListAllowlistResponse,
    ListBlocklistsResponse, ListDeviceFilterSettingsParams, ListDeviceFilterSettingsResponse,
    ListFilterRulesResponse, ListProfilesResponse, UpdateBlocklistRequest, UpdateBlocklistResponse,
    UpdateDeviceFilterSettingsRequest, UpdateDeviceFilterSettingsResponse,
    UpdateDnsFilterConfigRequest, UpdateFilterRuleRequest, UpdateFilterRuleResponse,
    UpdateProfileRequest, UpdateProfileResponse,
};
use wardnet_common::jobs::JobDispatchedResponse;

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_profiles, create_profile))
        .routes(routes!(get_profile, update_profile, delete_profile))
        .routes(routes!(list_blocklists, create_blocklist))
        .routes(routes!(update_blocklist, delete_blocklist))
        .routes(routes!(refresh_blocklist))
        .routes(routes!(list_allowlist, create_allowlist_entry))
        .routes(routes!(delete_allowlist_entry))
        .routes(routes!(list_custom_rules, create_custom_rule))
        .routes(routes!(update_custom_rule, delete_custom_rule))
        .routes(routes!(list_device_settings))
        .routes(routes!(get_device_settings, update_device_settings))
        .routes(routes!(get_filter_config, update_filter_config))
}

// ── Profiles ────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/dns/filter/profiles",
    tag = "dns",
    description = "List every DNS filter profile. Admin only.",
    responses(
        (status = 200, description = "Profiles list", body = ListProfilesResponse),
        AuthErrors,
    ),
)]
pub async fn list_profiles(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListProfilesResponse>, AppError> {
    Ok(Json(state.dns_filter_service().list_profiles().await?))
}

#[utoipa::path(
    post,
    path = "/api/dns/filter/profiles",
    tag = "dns",
    description = "Create a new DNS filter profile. Admin only.",
    request_body = CreateProfileRequest,
    responses(
        (status = 201, description = "Profile created", body = CreateProfileResponse),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn create_profile(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateProfileRequest>,
) -> Result<(StatusCode, Json<CreateProfileResponse>), AppError> {
    let response = state.dns_filter_service().create_profile(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/api/dns/filter/profiles/{profile_id}",
    tag = "dns",
    description = "Fetch a single DNS filter profile. Admin only.",
    params(("profile_id" = Uuid, Path, description = "Profile id")),
    responses(
        (status = 200, description = "Profile", body = GetProfileResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn get_profile(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<GetProfileResponse>, AppError> {
    Ok(Json(
        state.dns_filter_service().get_profile(profile_id).await?,
    ))
}

#[utoipa::path(
    put,
    path = "/api/dns/filter/profiles/{profile_id}",
    tag = "dns",
    description = "Rename a DNS filter profile. Admin only.",
    params(("profile_id" = Uuid, Path, description = "Profile id")),
    request_body = UpdateProfileRequest,
    responses(
        (status = 200, description = "Updated profile", body = UpdateProfileResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn update_profile(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(profile_id): Path<Uuid>,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<UpdateProfileResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .update_profile(profile_id, body)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/dns/filter/profiles/{profile_id}",
    tag = "dns",
    description = "Delete a non-builtin DNS filter profile. Returns 409 for builtin \
                   profiles (Ad Blocking, Parental Controls, Malware & Phishing). \
                   Admin only.",
    params(("profile_id" = Uuid, Path, description = "Profile id")),
    responses(
        (status = 200, description = "Profile deleted", body = DeleteProfileResponse),
        (status = 409, description = "Profile is builtin and cannot be deleted"),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn delete_profile(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<DeleteProfileResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .delete_profile(profile_id)
            .await?,
    ))
}

// ── Blocklists ─────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/dns/filter/profiles/{profile_id}/blocklists",
    tag = "dns",
    description = "List every blocklist scoped to a profile. Admin only.",
    params(("profile_id" = Uuid, Path, description = "Profile id")),
    responses(
        (status = 200, description = "Blocklists", body = ListBlocklistsResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn list_blocklists(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<ListBlocklistsResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .list_blocklists(profile_id)
            .await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/dns/filter/profiles/{profile_id}/blocklists",
    tag = "dns",
    description = "Create a blocklist under a profile. Admin only.",
    params(("profile_id" = Uuid, Path, description = "Profile id")),
    request_body = CreateBlocklistRequest,
    responses(
        (status = 201, description = "Blocklist created", body = CreateBlocklistResponse),
        AuthErrors,
        BadRequest,
        NotFound,
    ),
)]
pub async fn create_blocklist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(profile_id): Path<Uuid>,
    Json(body): Json<CreateBlocklistRequest>,
) -> Result<(StatusCode, Json<CreateBlocklistResponse>), AppError> {
    let response = state
        .dns_filter_service()
        .create_blocklist(profile_id, body)
        .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    put,
    path = "/api/dns/filter/profiles/{profile_id}/blocklists/{id}",
    tag = "dns",
    description = "Update a blocklist within a profile. Admin only.",
    params(
        ("profile_id" = Uuid, Path, description = "Profile id"),
        ("id" = Uuid, Path, description = "Blocklist id"),
    ),
    request_body = UpdateBlocklistRequest,
    responses(
        (status = 200, description = "Updated blocklist", body = UpdateBlocklistResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn update_blocklist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path((profile_id, id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateBlocklistRequest>,
) -> Result<Json<UpdateBlocklistResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .update_blocklist(profile_id, id, body)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/dns/filter/profiles/{profile_id}/blocklists/{id}",
    tag = "dns",
    description = "Delete a blocklist from a profile. Admin only.",
    params(
        ("profile_id" = Uuid, Path, description = "Profile id"),
        ("id" = Uuid, Path, description = "Blocklist id"),
    ),
    responses(
        (status = 200, description = "Blocklist deleted", body = DeleteBlocklistResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn delete_blocklist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path((profile_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DeleteBlocklistResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .delete_blocklist(profile_id, id)
            .await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/dns/filter/profiles/{profile_id}/blocklists/{id}/refresh",
    tag = "dns",
    description = "Trigger an immediate blocklist refresh job. Admin only.",
    params(
        ("profile_id" = Uuid, Path, description = "Profile id"),
        ("id" = Uuid, Path, description = "Blocklist id"),
    ),
    responses(
        (status = 202, description = "Refresh job dispatched", body = JobDispatchedResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn refresh_blocklist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path((profile_id, id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<JobDispatchedResponse>), AppError> {
    let response = state
        .dns_filter_service()
        .refresh_blocklist(profile_id, id)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(response)))
}

// ── Allowlist ──────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/dns/filter/profiles/{profile_id}/allowlist",
    tag = "dns",
    description = "List allowlist entries scoped to a profile. Admin only.",
    params(("profile_id" = Uuid, Path, description = "Profile id")),
    responses(
        (status = 200, description = "Allowlist", body = ListAllowlistResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn list_allowlist(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<ListAllowlistResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .list_allowlist(profile_id)
            .await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/dns/filter/profiles/{profile_id}/allowlist",
    tag = "dns",
    description = "Add a domain to the profile's allowlist. Admin only.",
    params(("profile_id" = Uuid, Path, description = "Profile id")),
    request_body = CreateAllowlistRequest,
    responses(
        (status = 201, description = "Allowlist entry", body = CreateAllowlistResponse),
        AuthErrors,
        BadRequest,
        NotFound,
    ),
)]
pub async fn create_allowlist_entry(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(profile_id): Path<Uuid>,
    Json(body): Json<CreateAllowlistRequest>,
) -> Result<(StatusCode, Json<CreateAllowlistResponse>), AppError> {
    let response = state
        .dns_filter_service()
        .create_allowlist_entry(profile_id, body)
        .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    delete,
    path = "/api/dns/filter/profiles/{profile_id}/allowlist/{id}",
    tag = "dns",
    description = "Remove a domain from a profile's allowlist. Admin only.",
    params(
        ("profile_id" = Uuid, Path, description = "Profile id"),
        ("id" = Uuid, Path, description = "Allowlist entry id"),
    ),
    responses(
        (status = 200, description = "Allowlist entry deleted", body = DeleteAllowlistResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn delete_allowlist_entry(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path((profile_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DeleteAllowlistResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .delete_allowlist_entry(profile_id, id)
            .await?,
    ))
}

// ── Custom rules ───────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/dns/filter/profiles/{profile_id}/rules",
    tag = "dns",
    description = "List a profile's custom filter rules. Admin only.",
    params(("profile_id" = Uuid, Path, description = "Profile id")),
    responses(
        (status = 200, description = "Filter rules", body = ListFilterRulesResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn list_custom_rules(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(profile_id): Path<Uuid>,
) -> Result<Json<ListFilterRulesResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .list_custom_rules(profile_id)
            .await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/dns/filter/profiles/{profile_id}/rules",
    tag = "dns",
    description = "Add a custom filter rule to a profile. Admin only.",
    params(("profile_id" = Uuid, Path, description = "Profile id")),
    request_body = CreateFilterRuleRequest,
    responses(
        (status = 201, description = "Filter rule created", body = CreateFilterRuleResponse),
        AuthErrors,
        BadRequest,
        NotFound,
    ),
)]
pub async fn create_custom_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(profile_id): Path<Uuid>,
    Json(body): Json<CreateFilterRuleRequest>,
) -> Result<(StatusCode, Json<CreateFilterRuleResponse>), AppError> {
    let response = state
        .dns_filter_service()
        .create_custom_rule(profile_id, body)
        .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    put,
    path = "/api/dns/filter/profiles/{profile_id}/rules/{id}",
    tag = "dns",
    description = "Update a custom filter rule. Admin only.",
    params(
        ("profile_id" = Uuid, Path, description = "Profile id"),
        ("id" = Uuid, Path, description = "Rule id"),
    ),
    request_body = UpdateFilterRuleRequest,
    responses(
        (status = 200, description = "Updated rule", body = UpdateFilterRuleResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn update_custom_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path((profile_id, id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateFilterRuleRequest>,
) -> Result<Json<UpdateFilterRuleResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .update_custom_rule(profile_id, id, body)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/dns/filter/profiles/{profile_id}/rules/{id}",
    tag = "dns",
    description = "Delete a custom filter rule from a profile. Admin only.",
    params(
        ("profile_id" = Uuid, Path, description = "Profile id"),
        ("id" = Uuid, Path, description = "Rule id"),
    ),
    responses(
        (status = 200, description = "Rule deleted", body = DeleteFilterRuleResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn delete_custom_rule(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path((profile_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<DeleteFilterRuleResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .delete_custom_rule(profile_id, id)
            .await?,
    ))
}

// ── Per-device settings ────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/dns/filter/devices",
    tag = "dns",
    description = "List devices with explicit DNS filter settings or profile \
                   assignments. Use `?enabled=false` to filter to devices where the \
                   per-device kill switch is off. Admin only.",
    params(ListDeviceFilterSettingsParams),
    responses(
        (status = 200, description = "Device settings", body = ListDeviceFilterSettingsResponse),
        AuthErrors,
    ),
)]
pub async fn list_device_settings(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Query(params): Query<ListDeviceFilterSettingsParams>,
) -> Result<Json<ListDeviceFilterSettingsResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .list_device_settings(params)
            .await?,
    ))
}

#[utoipa::path(
    get,
    path = "/api/dns/filter/devices/{device_id}",
    tag = "dns",
    description = "Get a device's DNS filter settings. Returns the default \
                   (enabled, no explicit profiles) for devices that have never \
                   been configured. Admin only.",
    params(("device_id" = Uuid, Path, description = "Device id")),
    responses(
        (status = 200, description = "Device settings", body = GetDeviceFilterSettingsResponse),
        AuthErrors,
    ),
)]
pub async fn get_device_settings(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(device_id): Path<Uuid>,
) -> Result<Json<GetDeviceFilterSettingsResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .get_device_settings(device_id)
            .await?,
    ))
}

#[utoipa::path(
    put,
    path = "/api/dns/filter/devices/{device_id}",
    tag = "dns",
    description = "Update a device's DNS filter settings (kill switch + profile \
                   assignments). Admin only.",
    params(("device_id" = Uuid, Path, description = "Device id")),
    request_body = UpdateDeviceFilterSettingsRequest,
    responses(
        (status = 200, description = "Updated settings", body = UpdateDeviceFilterSettingsResponse),
        AuthErrors,
        BadRequest,
        NotFound,
    ),
)]
pub async fn update_device_settings(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(device_id): Path<Uuid>,
    Json(body): Json<UpdateDeviceFilterSettingsRequest>,
) -> Result<Json<UpdateDeviceFilterSettingsResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .update_device_settings(device_id, body)
            .await?,
    ))
}

// ── Global config ──────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/dns/filter/config",
    tag = "dns",
    description = "Read the global DNS filter config (emergency stop + default \
                   profile pointer). Admin only.",
    responses(
        (status = 200, description = "Filter config", body = DnsFilterConfigResponse),
        AuthErrors,
    ),
)]
pub async fn get_filter_config(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<DnsFilterConfigResponse>, AppError> {
    Ok(Json(state.dns_filter_service().get_filter_config().await?))
}

#[utoipa::path(
    put,
    path = "/api/dns/filter/config",
    tag = "dns",
    description = "Update the global DNS filter config. Admin only.",
    request_body = UpdateDnsFilterConfigRequest,
    responses(
        (status = 200, description = "Updated config", body = DnsFilterConfigResponse),
        AuthErrors,
        BadRequest,
        NotFound,
    ),
)]
pub async fn update_filter_config(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<UpdateDnsFilterConfigRequest>,
) -> Result<Json<DnsFilterConfigResponse>, AppError> {
    Ok(Json(
        state
            .dns_filter_service()
            .update_filter_config(body)
            .await?,
    ))
}
