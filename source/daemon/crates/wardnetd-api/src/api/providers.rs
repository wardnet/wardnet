use axum::Json;
use axum::extract::{Path, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{
    ListCountriesResponse, ListProvidersResponse, ListServersRequest, ListServersResponse,
    SetupProviderRequest, SetupProviderResponse, ValidateCredentialsRequest,
    ValidateCredentialsResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest, NotFound};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register VPN provider routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(list_providers))
        .routes(routes!(validate_credentials))
        .routes(routes!(list_countries))
        .routes(routes!(list_servers))
        .routes(routes!(setup_tunnel))
}

#[utoipa::path(
    get,
    path = "/api/providers",
    tag = "providers",
    description = "List every VPN provider the daemon knows how to integrate with \
                   (e.g. NordVPN), along with the capabilities each provider exposes. \
                   Admin only.",
    responses(
        (status = 200, description = "Registered VPN providers", body = ListProvidersResponse),
        AuthErrors,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_providers(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListProvidersResponse>, AppError> {
    let response = state.provider_service().list_providers().await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/providers/{id}/validate",
    tag = "providers",
    description = "Validate a set of provider credentials by attempting a live auth \
                   call against the provider's API. Used by the guided setup wizard \
                   before persisting credentials. Admin only.",
    params(("id" = String, Path, description = "Provider ID")),
    request_body = ValidateCredentialsRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateCredentialsResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn validate_credentials(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
    Json(body): Json<ValidateCredentialsRequest>,
) -> Result<Json<ValidateCredentialsResponse>, AppError> {
    let response = state
        .provider_service()
        .validate_credentials(&id, body)
        .await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/providers/{id}/countries",
    tag = "providers",
    description = "List the countries in which a given VPN provider offers servers. \
                   Used by the setup wizard's country picker. Admin only.",
    params(("id" = String, Path, description = "Provider ID")),
    responses(
        (status = 200, description = "Countries with available servers", body = ListCountriesResponse),
        AuthErrors,
        NotFound,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_countries(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
) -> Result<Json<ListCountriesResponse>, AppError> {
    let response = state.provider_service().list_countries(&id).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/providers/{id}/servers",
    tag = "providers",
    description = "List the available VPN servers for a provider, optionally filtered \
                   by country. Credentials are supplied in the request body because \
                   some providers gate their server catalogue behind authentication. \
                   Admin only.",
    params(("id" = String, Path, description = "Provider ID")),
    request_body = ListServersRequest,
    responses(
        (status = 200, description = "Available servers", body = ListServersResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn list_servers(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
    Json(body): Json<ListServersRequest>,
) -> Result<Json<ListServersResponse>, AppError> {
    let response = state.provider_service().list_servers(&id, body).await?;
    Ok(Json(response))
}

#[utoipa::path(
    post,
    path = "/api/providers/{id}/setup",
    tag = "providers",
    description = "Run the full guided tunnel setup for a VPN provider end to end: \
                   validate credentials, generate a WireGuard key pair via the \
                   provider's API, materialise the resulting tunnel, and persist it. \
                   Admin only.",
    params(("id" = String, Path, description = "Provider ID")),
    request_body = SetupProviderRequest,
    responses(
        (status = 200, description = "Guided tunnel setup result", body = SetupProviderResponse),
        AuthErrors,
        NotFound,
        BadRequest,
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn setup_tunnel(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
    Json(body): Json<SetupProviderRequest>,
) -> Result<Json<SetupProviderResponse>, AppError> {
    let response = state.provider_service().setup_tunnel(&id, body).await?;
    Ok(Json(response))
}
