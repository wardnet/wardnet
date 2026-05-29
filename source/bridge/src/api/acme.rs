use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::AuthenticatedInstall;
use crate::error::ApiError;
use crate::state::AppState;

/// Register ACME challenge routes.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(set_acme_challenge))
        .routes(routes!(delete_acme_challenge))
}

/// Request body for `PUT /v1/installs/{id}/acme-challenge`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetAcmeChallengeRequest {
    /// The ACME DNS-01 challenge token value (raw, no quoting needed).
    pub value: String,
}

#[utoipa::path(
    put,
    path = "/v1/installs/{id}/acme-challenge",
    tag = "installs",
    description = "Set the DNS-01 ACME challenge TXT record for this installation. \
                   Creates `_acme-challenge.<name>.my.<region>.wardnet.network`. \
                   \n\n\
                   Called by the daemon's `AcmeManager` before presenting the DNS-01 \
                   challenge to Let's Encrypt. The daemon must wait for DNS propagation \
                   before completing the ACME order.",
    params(
        ("id" = String, Path, description = "Installation UUID"),
    ),
    request_body = SetAcmeChallengeRequest,
    responses(
        (status = 204, description = "TXT record created or updated"),
        (status = 401, description = "Authentication required or invalid"),
        (status = 403, description = "Bearer token does not own this install ID"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn set_acme_challenge(
    State(state): State<AppState>,
    Path(id): Path<String>,
    AuthenticatedInstall(install): AuthenticatedInstall,
    Json(body): Json<SetAcmeChallengeRequest>,
) -> Result<StatusCode, ApiError> {
    if install.id != id {
        return Err(ApiError::Forbidden(
            "bearer token does not match the requested install ID".to_string(),
        ));
    }

    let fqdn = state.config().acme_fqdn(&install.name);

    let record_id = state
        .dns()
        .upsert_txt_record(&fqdn, &body.value, install.cf_acme_record_id.as_deref())
        .await
        .map_err(|e| {
            tracing::error!(install_id = %id, error = %e, "Cloudflare ACME TXT upsert failed");
            ApiError::Internal(e)
        })?;

    state
        .installs()
        .update_acme_record(&id, Some(&record_id), Utc::now())
        .await
        .map_err(ApiError::Internal)?;

    tracing::info!(install_id = %id, fqdn = %fqdn, "ACME TXT record set");
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/v1/installs/{id}/acme-challenge",
    tag = "installs",
    description = "Remove the DNS-01 ACME challenge TXT record for this installation. \
                   Called by the daemon's `AcmeManager` after Let's Encrypt has \
                   completed DNS-01 validation. Idempotent — safe to call even if no \
                   TXT record is currently set.",
    params(
        ("id" = String, Path, description = "Installation UUID"),
    ),
    responses(
        (status = 204, description = "TXT record deleted (or was already absent)"),
        (status = 401, description = "Authentication required or invalid"),
        (status = 403, description = "Bearer token does not own this install ID"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn delete_acme_challenge(
    State(state): State<AppState>,
    Path(id): Path<String>,
    AuthenticatedInstall(install): AuthenticatedInstall,
) -> Result<StatusCode, ApiError> {
    if install.id != id {
        return Err(ApiError::Forbidden(
            "bearer token does not match the requested install ID".to_string(),
        ));
    }

    if let Some(record_id) = &install.cf_acme_record_id {
        state.dns().delete_record(record_id).await.map_err(|e| {
            tracing::error!(install_id = %id, error = %e, "Cloudflare ACME TXT delete failed");
            ApiError::Internal(e)
        })?;

        state
            .installs()
            .update_acme_record(&id, None, Utc::now())
            .await
            .map_err(ApiError::Internal)?;

        tracing::info!(install_id = %id, "ACME TXT record deleted");
    }

    Ok(StatusCode::NO_CONTENT)
}

// Full-stack ACME-challenge tests live in tests/api.rs.
