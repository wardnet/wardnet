use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::access_request::{
    AccessRequestStatus, CreateAccessRequestRequest, DecideAccessRequestRequest,
    DeviceAccessRequest,
};
use wardnet_common::api::ApiError;

use crate::api::middleware::{ClientIp, SessionAuth};
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

const TAG: &str = "access-requests";
const PATH_ME: &str = "/api/devices/me/access-requests";
const PATH_ADMIN: &str = "/api/access-requests";
const PATH_ADMIN_ID: &str = "/api/access-requests/{id}";

/// Register access-request routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(create_my_access_request, list_my_access_requests))
        .routes(routes!(list_access_requests))
        .routes(routes!(decide_access_request))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListAccessRequestsQuery {
    /// Optional status filter (`pending` | `approved` | `rejected`).
    status: Option<AccessRequestStatus>,
}

#[utoipa::path(
    post,
    path = PATH_ME,
    tag = TAG,
    description = "Ask an admin for something this device cannot grant itself: \
                   a domain blocked or allowed (`block` / `allow`, which take a \
                   `domain`), or Private DNS access (`private_dns`, which takes \
                   none). The device is identified by source IP — no \
                   authentication required.",
    request_body = CreateAccessRequestRequest,
    responses(
        (status = 201, description = "Request created", body = DeviceAccessRequest),
        (status = 400, description = "Malformed request", body = ApiError),
        (status = 404, description = "Device not found for this IP", body = ApiError),
        (status = 409, description = "Already pending, or the feature is not enabled", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn create_my_access_request(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<CreateAccessRequestRequest>,
) -> Result<(StatusCode, Json<DeviceAccessRequest>), AppError> {
    let created = state
        .access_request_service()
        .create_for_ip(&ip.to_string(), body.kind, body.domain, body.reason)
        .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = PATH_ME,
    tag = TAG,
    description = "List the access requests made by this device (by source IP), \
                   newest first.",
    responses(
        (status = 200, description = "The caller's access requests", body = [DeviceAccessRequest]),
        (status = 404, description = "Device not found for this IP", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn list_my_access_requests(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
) -> Result<Json<Vec<DeviceAccessRequest>>, AppError> {
    let requests = state
        .access_request_service()
        .list_for_ip(&ip.to_string())
        .await?;
    Ok(Json(requests))
}

#[utoipa::path(
    get,
    path = PATH_ADMIN,
    tag = TAG,
    params(ListAccessRequestsQuery),
    description = "List all device access requests, optionally filtered by \
                   status. Admin only.",
    responses(
        (status = 200, description = "Access requests", body = [DeviceAccessRequest]),
        AuthErrors,
        (status = 500, description = "Internal server error", body = ApiError),
    ),
)]
pub async fn list_access_requests(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Query(query): Query<ListAccessRequestsQuery>,
) -> Result<Json<Vec<DeviceAccessRequest>>, AppError> {
    let requests = state.access_request_service().list(query.status).await?;
    Ok(Json(requests))
}

#[utoipa::path(
    patch,
    path = PATH_ADMIN_ID,
    tag = TAG,
    params(("id" = String, Path, description = "Access request id")),
    description = "Approve or reject an access request. What approval *does* \
                   depends on the request's kind: approving a `private_dns` \
                   request mints the device's grant, while `allow` / `block` \
                   only record the decision — the admin still applies the DNS \
                   filter rule via the DNS filter UI. Admin only.",
    request_body = DecideAccessRequestRequest,
    responses(
        (status = 200, description = "Updated request", body = DeviceAccessRequest),
        AuthErrors,
        BadRequest,
        NotFound,
        (status = 409, description = "Already decided, or the approval could not be carried out", body = ApiError),
    ),
)]
pub async fn decide_access_request(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Path(id): Path<String>,
    Json(body): Json<DecideAccessRequestRequest>,
) -> Result<Json<DeviceAccessRequest>, AppError> {
    let updated = state
        .access_request_service()
        .decide(&id, body.status, body.approval)
        .await?;
    Ok(Json(updated))
}
