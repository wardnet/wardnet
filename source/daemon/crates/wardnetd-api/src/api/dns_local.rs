//! Local DNS HTTP handlers — `/api/dns/local/...` (issue #217).
//!
//! CRUD over authoritative zones, custom records, and forwarding rules
//! (conditional forwarding). Parallel to [`super::dns_filter`]; the DNS server
//! lifecycle stays in [`super::dns`].

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    CreateForwardingRuleRequest, CreateForwardingRuleResponse, CreateRecordRequest,
    CreateRecordResponse, CreateZoneRequest, CreateZoneResponse, DeleteForwardingRuleResponse,
    DeleteRecordResponse, DeleteZoneResponse, GetForwardingRuleResponse, GetRecordResponse,
    GetZoneResponse, ListForwardingRulesResponse, ListRecordsResponse, ListZonesResponse,
    UpdateForwardingRuleRequest, UpdateForwardingRuleResponse, UpdateRecordRequest,
    UpdateRecordResponse, UpdateZoneRequest, UpdateZoneResponse,
};

use crate::api::middleware::SessionAuth;
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_zones, create_zone))
        .routes(routes!(get_zone, update_zone, delete_zone))
        .routes(routes!(list_zone_records))
        .routes(routes!(list_records, create_record))
        .routes(routes!(get_record, update_record, delete_record))
        .routes(routes!(list_forwarding_rules, create_forwarding_rule))
        .routes(routes!(
            get_forwarding_rule,
            update_forwarding_rule,
            delete_forwarding_rule
        ))
}

// ── Zones ──────────────────────────────────────────────────────────────

#[utoipa::path(
    operation_id = "dns_local_list_zones",
    get,
    path = "/api/dns/local/zones",
    tag = "dns",
    description = "List every authoritative local DNS zone. Admin only.",
    responses(
        (status = 200, description = "Zones list", body = ListZonesResponse),
        AuthErrors,
    ),
)]
pub async fn list_zones(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<ListZonesResponse>, AppError> {
    Ok(Json(state.dns_local_service().list_zones().await?))
}

#[utoipa::path(
    operation_id = "dns_local_create_zone",
    post,
    path = "/api/dns/local/zones",
    tag = "dns",
    description = "Create an authoritative local DNS zone. Admin only.",
    request_body = CreateZoneRequest,
    responses(
        (status = 201, description = "Zone created", body = CreateZoneResponse),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn create_zone(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Json(body): Json<CreateZoneRequest>,
) -> Result<(StatusCode, Json<CreateZoneResponse>), AppError> {
    let response = state.dns_local_service().create_zone(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    operation_id = "dns_local_get_zone",
    get,
    path = "/api/dns/local/zones/{id}",
    tag = "dns",
    description = "Fetch a single local DNS zone. Admin only.",
    params(("id" = Uuid, Path, description = "Zone id")),
    responses(
        (status = 200, description = "Zone", body = GetZoneResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn get_zone(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<GetZoneResponse>, AppError> {
    Ok(Json(state.dns_local_service().get_zone(id).await?))
}

#[utoipa::path(
    operation_id = "dns_local_update_zone",
    put,
    path = "/api/dns/local/zones/{id}",
    tag = "dns",
    description = "Partially update a local DNS zone. Admin only.",
    params(("id" = Uuid, Path, description = "Zone id")),
    request_body = UpdateZoneRequest,
    responses(
        (status = 200, description = "Updated zone", body = UpdateZoneResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn update_zone(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateZoneRequest>,
) -> Result<Json<UpdateZoneResponse>, AppError> {
    Ok(Json(state.dns_local_service().update_zone(id, body).await?))
}

#[utoipa::path(
    operation_id = "dns_local_delete_zone",
    delete,
    path = "/api/dns/local/zones/{id}",
    tag = "dns",
    description = "Delete a local DNS zone. Records keep their data but lose \
                   the zone link (set to NULL). Admin only.",
    params(("id" = Uuid, Path, description = "Zone id")),
    responses(
        (status = 200, description = "Zone deleted", body = DeleteZoneResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn delete_zone(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteZoneResponse>, AppError> {
    Ok(Json(state.dns_local_service().delete_zone(id).await?))
}

#[utoipa::path(
    get,
    path = "/api/dns/local/zones/{id}/records",
    tag = "dns",
    description = "List the custom records assigned to a zone. Admin only.",
    params(("id" = Uuid, Path, description = "Zone id")),
    responses(
        (status = 200, description = "Records list", body = ListRecordsResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn list_zone_records(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<ListRecordsResponse>, AppError> {
    Ok(Json(state.dns_local_service().list_zone_records(id).await?))
}

// ── Records ────────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/dns/local/records",
    tag = "dns",
    description = "List every custom local DNS record. Admin only.",
    responses(
        (status = 200, description = "Records list", body = ListRecordsResponse),
        AuthErrors,
    ),
)]
pub async fn list_records(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<ListRecordsResponse>, AppError> {
    Ok(Json(state.dns_local_service().list_records().await?))
}

#[utoipa::path(
    post,
    path = "/api/dns/local/records",
    tag = "dns",
    description = "Create a custom local DNS record. Admin only.",
    request_body = CreateRecordRequest,
    responses(
        (status = 201, description = "Record created", body = CreateRecordResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn create_record(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Json(body): Json<CreateRecordRequest>,
) -> Result<(StatusCode, Json<CreateRecordResponse>), AppError> {
    let response = state.dns_local_service().create_record(body).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/api/dns/local/records/{id}",
    tag = "dns",
    description = "Fetch a single custom local DNS record. Admin only.",
    params(("id" = Uuid, Path, description = "Record id")),
    responses(
        (status = 200, description = "Record", body = GetRecordResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn get_record(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<GetRecordResponse>, AppError> {
    Ok(Json(state.dns_local_service().get_record(id).await?))
}

#[utoipa::path(
    put,
    path = "/api/dns/local/records/{id}",
    tag = "dns",
    description = "Partially update a custom local DNS record. Admin only.",
    params(("id" = Uuid, Path, description = "Record id")),
    request_body = UpdateRecordRequest,
    responses(
        (status = 200, description = "Updated record", body = UpdateRecordResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn update_record(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateRecordRequest>,
) -> Result<Json<UpdateRecordResponse>, AppError> {
    Ok(Json(
        state.dns_local_service().update_record(id, body).await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/dns/local/records/{id}",
    tag = "dns",
    description = "Delete a custom local DNS record. Admin only.",
    params(("id" = Uuid, Path, description = "Record id")),
    responses(
        (status = 200, description = "Record deleted", body = DeleteRecordResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn delete_record(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteRecordResponse>, AppError> {
    Ok(Json(state.dns_local_service().delete_record(id).await?))
}

// ── Forwarding rules ─────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/api/dns/local/forwarding",
    tag = "dns",
    description = "List every conditional-forwarding rule (domain → upstream). \
                   Admin only.",
    responses(
        (status = 200, description = "Forwarding rules list", body = ListForwardingRulesResponse),
        AuthErrors,
    ),
)]
pub async fn list_forwarding_rules(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<ListForwardingRulesResponse>, AppError> {
    Ok(Json(
        state.dns_local_service().list_forwarding_rules().await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/dns/local/forwarding",
    tag = "dns",
    description = "Create a conditional-forwarding rule. Admin only.",
    request_body = CreateForwardingRuleRequest,
    responses(
        (status = 201, description = "Forwarding rule created", body = CreateForwardingRuleResponse),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn create_forwarding_rule(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Json(body): Json<CreateForwardingRuleRequest>,
) -> Result<(StatusCode, Json<CreateForwardingRuleResponse>), AppError> {
    let response = state
        .dns_local_service()
        .create_forwarding_rule(body)
        .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

#[utoipa::path(
    get,
    path = "/api/dns/local/forwarding/{id}",
    tag = "dns",
    description = "Fetch a single conditional-forwarding rule. Admin only.",
    params(("id" = Uuid, Path, description = "Forwarding rule id")),
    responses(
        (status = 200, description = "Forwarding rule", body = GetForwardingRuleResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn get_forwarding_rule(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<GetForwardingRuleResponse>, AppError> {
    Ok(Json(
        state.dns_local_service().get_forwarding_rule(id).await?,
    ))
}

#[utoipa::path(
    put,
    path = "/api/dns/local/forwarding/{id}",
    tag = "dns",
    description = "Partially update a conditional-forwarding rule. Admin only.",
    params(("id" = Uuid, Path, description = "Forwarding rule id")),
    request_body = UpdateForwardingRuleRequest,
    responses(
        (status = 200, description = "Updated forwarding rule", body = UpdateForwardingRuleResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
)]
pub async fn update_forwarding_rule(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateForwardingRuleRequest>,
) -> Result<Json<UpdateForwardingRuleResponse>, AppError> {
    Ok(Json(
        state
            .dns_local_service()
            .update_forwarding_rule(id, body)
            .await?,
    ))
}

#[utoipa::path(
    delete,
    path = "/api/dns/local/forwarding/{id}",
    tag = "dns",
    description = "Delete a conditional-forwarding rule. Admin only.",
    params(("id" = Uuid, Path, description = "Forwarding rule id")),
    responses(
        (status = 200, description = "Forwarding rule deleted", body = DeleteForwardingRuleResponse),
        AuthErrors,
        NotFound,
    ),
)]
pub async fn delete_forwarding_rule(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteForwardingRuleResponse>, AppError> {
    Ok(Json(
        state.dns_local_service().delete_forwarding_rule(id).await?,
    ))
}
