use axum::Json;
use axum::extract::{Path, State};
use wardnet_common::api::{
    ListCountriesResponse, ListProvidersResponse, ListServersRequest, ListServersResponse,
    SetupProviderRequest, SetupProviderResponse, ValidateCredentialsRequest,
    ValidateCredentialsResponse,
};

use crate::api::middleware::AdminAuth;
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// GET /api/providers -- list all registered VPN providers.
pub async fn list_providers(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<ListProvidersResponse>, AppError> {
    let response = state.provider_service().list_providers().await?;
    Ok(Json(response))
}

/// POST /api/providers/{id}/validate -- validate credentials for a provider.
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

/// GET /api/providers/{id}/countries -- list countries where a provider has servers.
pub async fn list_countries(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
) -> Result<Json<ListCountriesResponse>, AppError> {
    let response = state.provider_service().list_countries(&id).await?;
    Ok(Json(response))
}

/// POST /api/providers/{id}/servers -- list available servers (POST because body contains credentials).
pub async fn list_servers(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
    Json(body): Json<ListServersRequest>,
) -> Result<Json<ListServersResponse>, AppError> {
    let response = state.provider_service().list_servers(&id, body).await?;
    Ok(Json(response))
}

/// POST /api/providers/{id}/setup -- full guided tunnel setup through a provider.
pub async fn setup_tunnel(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Path(id): Path<String>,
    Json(body): Json<SetupProviderRequest>,
) -> Result<Json<SetupProviderResponse>, AppError> {
    let response = state.provider_service().setup_tunnel(&id, body).await?;
    Ok(Json(response))
}
