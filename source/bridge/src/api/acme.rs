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
    /// The ACME DNS-01 challenge token values (raw, no quoting needed). A
    /// **per-user wildcard certificate** authorizes its apex and wildcard SANs
    /// through the same `_acme-challenge` name, so this carries one value per SAN
    /// (typically two) and they are published as that many TXT records at once.
    pub values: Vec<String>,
}

#[utoipa::path(
    put,
    path = "/v1/installs/{id}/acme-challenge",
    tag = "installs",
    description = "Set the DNS-01 ACME challenge TXT records for this installation. \
                   Creates one `_acme-challenge.<name>.my.wardnet.services` TXT record \
                   per supplied value (a per-user wildcard cert authorizes two SANs \
                   through the same name). \
                   \n\n\
                   Called by the daemon's `AcmeManager` before presenting the DNS-01 \
                   challenge to Let's Encrypt. The daemon must wait for DNS propagation \
                   before completing the ACME order.",
    params(
        ("id" = String, Path, description = "Installation UUID"),
    ),
    request_body = SetAcmeChallengeRequest,
    responses(
        (status = 204, description = "TXT records created"),
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

    // Bound the list before any Cloudflare write: each value fans out to a TXT
    // create against the region's shared zone, so an unchecked count from a
    // (merely authenticated) install is a cross-tenant DoS vector.
    crate::api::validation::validate_acme_values(&body.values)?;

    let fqdn = state.config().acme_fqdn(&install.name);

    // Replace semantics: delete any records from a prior challenge first, then
    // create one per value. In the normal flow the daemon clears after every
    // issuance, so `cf_acme_record_ids` is virtually always empty here — this
    // loop is defensive cleanup for a previous clear that never completed. A
    // failed delete (other than already-absent, which the provider treats as
    // success) leaves the old IDs stored and is retried, so nothing is orphaned.
    for record_id in &install.cf_acme_record_ids {
        state.dns().delete_record(record_id).await.map_err(|e| {
            tracing::error!(install_id = %id, error = %e, "Cloudflare ACME TXT delete (stale) failed");
            ApiError::Internal(e)
        })?;
    }

    // Create the new TXT records. Each value is a fresh record (`None` =
    // create), so two differing values yield two co-existing TXT records at the
    // one name. On a partial failure, best-effort delete what we created and
    // persist the empty list — the old records are already gone, so this leaves
    // no live record untracked.
    let mut new_record_ids = Vec::with_capacity(body.values.len());
    for value in &body.values {
        match state.dns().upsert_txt_record(&fqdn, value, None).await {
            Ok(record_id) => new_record_ids.push(record_id),
            Err(e) => {
                for created in &new_record_ids {
                    if let Err(cleanup_err) = state.dns().delete_record(created).await {
                        tracing::warn!(
                            install_id = %id, record_id = %created, error = %cleanup_err,
                            "ACME TXT cleanup delete failed after a partial create; record may be orphaned in Cloudflare: {cleanup_err}"
                        );
                    }
                }
                state
                    .installs()
                    .set_acme_records(&id, &[], Utc::now())
                    .await
                    .map_err(ApiError::Internal)?;
                tracing::error!(install_id = %id, error = %e, "Cloudflare ACME TXT create failed");
                return Err(ApiError::Internal(e));
            }
        }
    }

    state
        .installs()
        .set_acme_records(&id, &new_record_ids, Utc::now())
        .await
        .map_err(ApiError::Internal)?;

    tracing::info!(
        install_id = %id, fqdn = %fqdn, count = new_record_ids.len(),
        "ACME TXT records set"
    );
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/v1/installs/{id}/acme-challenge",
    tag = "installs",
    description = "Remove the DNS-01 ACME challenge TXT records for this installation. \
                   Deletes every TXT record from the active challenge (a per-user \
                   wildcard cert publishes more than one). Called by the daemon's \
                   `AcmeManager` after Let's Encrypt has completed DNS-01 validation. \
                   Idempotent — safe to call even if no TXT record is currently set.",
    params(
        ("id" = String, Path, description = "Installation UUID"),
    ),
    responses(
        (status = 204, description = "TXT records deleted (or were already absent)"),
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

    if install.cf_acme_record_ids.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }

    for record_id in &install.cf_acme_record_ids {
        state.dns().delete_record(record_id).await.map_err(|e| {
            tracing::error!(install_id = %id, error = %e, "Cloudflare ACME TXT delete failed");
            ApiError::Internal(e)
        })?;
    }

    state
        .installs()
        .set_acme_records(&id, &[], Utc::now())
        .await
        .map_err(ApiError::Internal)?;

    tracing::info!(
        install_id = %id, count = install.cf_acme_record_ids.len(),
        "ACME TXT records deleted"
    );
    Ok(StatusCode::NO_CONTENT)
}

// Full-stack ACME-challenge tests live in tests/api.rs.
