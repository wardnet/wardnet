use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::ApiError;
use wardnet_common::rule_request::{
    CreateRuleRequestRequest, DecideRuleRequestRequest, DeviceRuleRequest, RuleRequestStatus,
};

use crate::api::middleware::{SessionAuth, ClientIp};
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

const TAG: &str = "rule-requests";
const PATH_ME: &str = "/api/devices/me/rule-requests";
const PATH_ADMIN: &str = "/api/rule-requests";
const PATH_ADMIN_ID: &str = "/api/rule-requests/{id}";

/// Register rule-request routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(create_my_rule_request, list_my_rule_requests))
        .routes(routes!(list_rule_requests))
        .routes(routes!(decide_rule_request))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListRuleRequestsQuery {
    /// Optional status filter (`pending` | `approved` | `rejected`).
    status: Option<RuleRequestStatus>,
}

#[utoipa::path(
    post,
    path = PATH_ME,
    tag = TAG,
    description = "Submit a request to the admin to block or allow a domain. \
                   The device is identified by source IP — no authentication \
                   required.",
    request_body = CreateRuleRequestRequest,
    responses(
        (status = 201, description = "Request created", body = DeviceRuleRequest),
        (status = 400, description = "Malformed request", body = ApiError),
        (status = 404, description = "Device not found for this IP", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn create_my_rule_request(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<CreateRuleRequestRequest>,
) -> Result<(StatusCode, Json<DeviceRuleRequest>), AppError> {
    let created = state
        .rule_request_service()
        .create_for_ip(&ip.to_string(), body.kind, &body.domain, body.reason)
        .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = PATH_ME,
    tag = TAG,
    description = "List the rule requests made by this device (by source IP), \
                   newest first.",
    responses(
        (status = 200, description = "The caller's rule requests", body = [DeviceRuleRequest]),
        (status = 404, description = "Device not found for this IP", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn list_my_rule_requests(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
) -> Result<Json<Vec<DeviceRuleRequest>>, AppError> {
    let requests = state
        .rule_request_service()
        .list_for_ip(&ip.to_string())
        .await?;
    Ok(Json(requests))
}

#[utoipa::path(
    get,
    path = PATH_ADMIN,
    tag = TAG,
    params(ListRuleRequestsQuery),
    description = "List all device rule requests, optionally filtered by status. \
                   Admin only.",
    responses(
        (status = 200, description = "Rule requests", body = [DeviceRuleRequest]),
        AuthErrors,
        (status = 500, description = "Internal server error", body = ApiError),
    ),
)]
pub async fn list_rule_requests(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Query(query): Query<ListRuleRequestsQuery>,
) -> Result<Json<Vec<DeviceRuleRequest>>, AppError> {
    let requests = state.rule_request_service().list(query.status).await?;
    Ok(Json(requests))
}

#[utoipa::path(
    patch,
    path = PATH_ADMIN_ID,
    tag = TAG,
    params(("id" = String, Path, description = "Rule request id")),
    description = "Approve or reject a rule request. Recording a decision does \
                   not apply the DNS rule — the admin applies it via the DNS \
                   filter UI. Admin only.",
    request_body = DecideRuleRequestRequest,
    responses(
        (status = 200, description = "Updated request", body = DeviceRuleRequest),
        AuthErrors,
        BadRequest,
        NotFound,
    ),
)]
pub async fn decide_rule_request(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
    Json(body): Json<DecideRuleRequestRequest>,
) -> Result<Json<DeviceRuleRequest>, AppError> {
    let updated = state
        .rule_request_service()
        .decide(&id, body.status)
        .await?;
    Ok(Json(updated))
}
