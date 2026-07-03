//! Cross-zone exception HTTP handlers — `/api/network/zones/exceptions/...`
//! (epic #244, issue #737).
//!
//! CRUD over the exception catalog: admin-granted allowances for one endpoint to
//! reach another across an otherwise-isolated zone boundary (e.g. a phone
//! casting to a TV). This commit is data-model + API only; the zone enforcer
//! consumes the exceptions in a later commit. All handlers are admin-only.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::api::{
    CreateZoneExceptionRequest, CreateZoneExceptionResponse, DeleteZoneExceptionResponse,
    GetZoneExceptionResponse, ListZoneExceptionsResponse, UpdateZoneExceptionRequest,
    UpdateZoneExceptionResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_exceptions, create_exception))
        .routes(routes!(get_exception, update_exception, delete_exception))
}

#[utoipa::path(
    get,
    path = "/api/network/zones/exceptions",
    tag = "zone_exceptions",
    description = "List every cross-zone exception. Admin only.",
    responses(
        (status = 200, description = "Exceptions list", body = ListZoneExceptionsResponse),
        AuthErrors,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn list_exceptions(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListZoneExceptionsResponse>, AppError> {
    let exceptions = state.zone_exception_service().list_exceptions().await?;
    Ok(Json(ListZoneExceptionsResponse { exceptions }))
}

#[utoipa::path(
    post,
    path = "/api/network/zones/exceptions",
    tag = "zone_exceptions",
    description = "Create a cross-zone exception. Both endpoints must exist and \
                   differ; a custom port list must be non-empty with valid \
                   ranges. Admin only.",
    request_body = CreateZoneExceptionRequest,
    responses(
        (status = 201, description = "Exception created", body = CreateZoneExceptionResponse),
        AuthErrors,
        BadRequest,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn create_exception(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<CreateZoneExceptionRequest>,
) -> Result<(StatusCode, Json<CreateZoneExceptionResponse>), AppError> {
    let exception = state
        .zone_exception_service()
        .create_exception(body)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateZoneExceptionResponse { exception }),
    ))
}

#[utoipa::path(
    get,
    path = "/api/network/zones/exceptions/{id}",
    tag = "zone_exceptions",
    description = "Fetch a single cross-zone exception. Admin only.",
    params(("id" = Uuid, Path, description = "Exception id")),
    responses(
        (status = 200, description = "Exception", body = GetZoneExceptionResponse),
        AuthErrors,
        NotFound,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn get_exception(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<GetZoneExceptionResponse>, AppError> {
    let exception = state.zone_exception_service().get_exception(id).await?;
    Ok(Json(GetZoneExceptionResponse { exception }))
}

#[utoipa::path(
    put,
    path = "/api/network/zones/exceptions/{id}",
    tag = "zone_exceptions",
    description = "Partially update a cross-zone exception. Every field is \
                   optional; the resolved exception is re-validated. Admin only.",
    params(("id" = Uuid, Path, description = "Exception id")),
    request_body = UpdateZoneExceptionRequest,
    responses(
        (status = 200, description = "Updated exception", body = UpdateZoneExceptionResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn update_exception(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateZoneExceptionRequest>,
) -> Result<Json<UpdateZoneExceptionResponse>, AppError> {
    let exception = state
        .zone_exception_service()
        .update_exception(id, body)
        .await?;
    Ok(Json(UpdateZoneExceptionResponse { exception }))
}

#[utoipa::path(
    delete,
    path = "/api/network/zones/exceptions/{id}",
    tag = "zone_exceptions",
    description = "Delete a cross-zone exception. Admin only.",
    params(("id" = Uuid, Path, description = "Exception id")),
    responses(
        (status = 200, description = "Exception deleted", body = DeleteZoneExceptionResponse),
        AuthErrors,
        NotFound,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn delete_exception(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteZoneExceptionResponse>, AppError> {
    state.zone_exception_service().delete_exception(id).await?;
    Ok(Json(DeleteZoneExceptionResponse { deleted: true }))
}
