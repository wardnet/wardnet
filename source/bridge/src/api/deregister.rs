use axum::extract::{Path, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::AuthenticatedInstall;
use crate::error::ApiError;
use crate::state::AppState;

/// Register the deregister route.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(deregister))
}

#[utoipa::path(
    delete,
    path = "/v1/installs/{id}",
    tag = "installs",
    description = "Deregister an installation. Deletes the Cloudflare A record and any \
                   active ACME TXT record immediately, then removes the install row. \
                   \n\n\
                   Idempotent on the DNS side — if Cloudflare records are already absent \
                   the delete calls are skipped gracefully.",
    params(
        ("id" = String, Path, description = "Installation UUID"),
    ),
    responses(
        (status = 204, description = "Installation deregistered"),
        (status = 401, description = "Authentication required or invalid"),
        (status = 403, description = "Bearer token does not own this install ID"),
        (status = 500, description = "Internal server error"),
    ),
)]
pub async fn deregister(
    State(state): State<AppState>,
    Path(id): Path<String>,
    AuthenticatedInstall(install): AuthenticatedInstall,
) -> Result<StatusCode, ApiError> {
    if install.id != id {
        return Err(ApiError::Forbidden(
            "bearer token does not match the requested install ID".to_string(),
        ));
    }

    // Delete A record.
    if let Some(record_id) = &install.cf_a_record_id {
        state.dns().delete_record(record_id).await.map_err(|e| {
            tracing::error!(install_id = %id, error = %e, "Cloudflare A-record delete failed on deregister");
            ApiError::Internal(e)
        })?;
    }

    // Delete ACME TXT record if one is live.
    if let Some(record_id) = &install.cf_acme_record_id {
        state.dns().delete_record(record_id).await.map_err(|e| {
            tracing::error!(install_id = %id, error = %e, "Cloudflare ACME TXT delete failed on deregister");
            ApiError::Internal(e)
        })?;
    }

    state
        .installs()
        .delete(&id)
        .await
        .map_err(ApiError::Internal)?;

    tracing::info!(install_id = %id, name = %install.name, "installation deregistered");
    Ok(StatusCode::NO_CONTENT)
}

// Full-stack deregister tests live in tests/api.rs.
