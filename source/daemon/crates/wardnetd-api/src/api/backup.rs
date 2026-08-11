//! `/api/backup/*` — export / preview-import / apply-import / snapshots.
//!
//! All endpoints are admin-guarded. Handlers are deliberately thin —
//! they call into [`BackupService`](wardnetd_services::BackupService)
//! which owns the crypto, filesystem, and pool-swap state machine.
//!
//! ### Binary bodies
//!
//! * `POST /api/backup/export` returns the `.wardnet.age` byte stream
//!   with `Content-Type: application/octet-stream` and
//!   `Content-Disposition: attachment; filename=...`. The passphrase
//!   travels in the JSON request body; the response body is binary.
//! * `POST /api/backup/import/preview` accepts a multipart upload
//!   (`bundle` file field + `passphrase` text field), validates the
//!   bundle against the running daemon, and returns a preview token.
//! * `POST /api/backup/import/apply` consumes the preview token.

use axum::Json;
use axum::body::Body;
use axum::extract::{Multipart, State};
use axum::http::header;
use axum::response::IntoResponse;
use chrono::Utc;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{
    ApplyImportRequest, ApplyImportResponse, BackupStatusResponse, ExportBackupRequest,
    ListSnapshotsResponse, RestorePreviewResponse,
};

use crate::api::middleware::SessionAuth;
use crate::api::responses::{AuthErrors, BadRequest};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register backup routes onto the given [`OpenApiRouter`].
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(status))
        .routes(routes!(export))
        .routes(routes!(preview_import))
        .routes(routes!(apply_import))
        .routes(routes!(list_snapshots))
}

#[utoipa::path(
    operation_id = "backup_status",
    get,
    path = "/api/backup/status",
    tag = "backup",
    description = "Return the current backup subsystem phase (idle, exporting, \
                   importing with a nested phase, or failed with a reason).",
    responses(
        (status = 200, description = "Current backup subsystem status", body = BackupStatusResponse),
        AuthErrors,
    ),
)]
pub async fn status(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<BackupStatusResponse>, AppError> {
    Ok(Json(state.backup_service().status().await?))
}

#[utoipa::path(
    post,
    path = "/api/backup/export",
    tag = "backup",
    description = "Produce an encrypted bundle and stream it back as \
                   `application/octet-stream`. The passphrase is required \
                   again on restore; the daemon cannot recover a forgotten \
                   one.",
    request_body = ExportBackupRequest,
    responses(
        (
            status = 200,
            description = "Encrypted .wardnet.age bundle",
            content_type = "application/octet-stream",
            body = Vec<u8>,
        ),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn export(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Json(body): Json<ExportBackupRequest>,
) -> Result<impl IntoResponse, AppError> {
    let bytes = state.backup_service().export(body).await?;

    let filename = format!(
        "wardnet-{timestamp}.wardnet.age",
        timestamp = Utc::now().format("%Y%m%dT%H%M%SZ")
    );
    let disposition = format!("attachment; filename=\"{filename}\"");

    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        Body::from(bytes),
    ))
}

#[utoipa::path(
    post,
    path = "/api/backup/import/preview",
    tag = "backup",
    description = "Accept a multipart upload with `bundle` (binary file) + \
                   `passphrase` (text), decrypt it server-side, validate \
                   compatibility against the running daemon, and return a \
                   short-lived preview token the caller can pass to \
                   `/api/backup/import/apply`. Nothing on disk changes yet.",
    request_body(
        content_type = "multipart/form-data",
        content = RestorePreviewRequest,
        description = "Bundle file (`bundle`) and passphrase (`passphrase`) as multipart fields",
    ),
    responses(
        (status = 200, description = "Preview token + bundle manifest", body = RestorePreviewResponse),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn preview_import(
    State(state): State<AppState>,
    _auth: SessionAuth,
    mut multipart: Multipart,
) -> Result<Json<RestorePreviewResponse>, AppError> {
    let (bundle, passphrase) = extract_multipart_fields(&mut multipart).await?;
    Ok(Json(
        state
            .backup_service()
            .preview_import(bundle, passphrase)
            .await?,
    ))
}

#[utoipa::path(
    post,
    path = "/api/backup/import/apply",
    tag = "backup",
    description = "Commit a previously-previewed import. Renames live files \
                   to `.bak-<timestamp>` siblings, writes the bundle \
                   contents into place, and sets `backup_restart_pending` \
                   in `system_config`. The daemon requires a restart \
                   afterwards for the live database pool to pick up the new \
                   file.",
    request_body = ApplyImportRequest,
    responses(
        (status = 200, description = "Restore applied, restart required", body = ApplyImportResponse),
        AuthErrors,
        BadRequest,
    ),
)]
pub async fn apply_import(
    State(state): State<AppState>,
    _auth: SessionAuth,
    Json(body): Json<ApplyImportRequest>,
) -> Result<Json<ApplyImportResponse>, AppError> {
    Ok(Json(state.backup_service().apply_import(body).await?))
}

#[utoipa::path(
    get,
    path = "/api/backup/snapshots",
    tag = "backup",
    description = "Enumerate `.bak-<timestamp>` siblings retained from prior \
                   restores. Snapshots are trimmed by a background runner \
                   after 24 hours.",
    responses(
        (status = 200, description = "Retained .bak-<ts> snapshots", body = ListSnapshotsResponse),
        AuthErrors,
    ),
)]
pub async fn list_snapshots(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<ListSnapshotsResponse>, AppError> {
    Ok(Json(state.backup_service().list_snapshots().await?))
}

/// Schema-only stand-in so the `OpenAPI` spec can describe the
/// multipart fields `preview_import` accepts. The handler itself
/// pulls fields out of [`axum::extract::Multipart`] directly — this
/// type is never materialised at runtime.
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[allow(dead_code)]
pub struct RestorePreviewRequest {
    /// The encrypted `.wardnet.age` bundle bytes.
    #[schema(format = Binary)]
    pub bundle: Vec<u8>,
    /// Passphrase that was used to encrypt the bundle on export.
    pub passphrase: String,
}

/// Pull the `bundle` and `passphrase` fields out of a multipart
/// request, validating both are present. Anything missing or
/// malformed surfaces as [`AppError::BadRequest`] with a precise
/// message — the UI renders those verbatim.
async fn extract_multipart_fields(
    multipart: &mut Multipart,
) -> Result<(Vec<u8>, String), AppError> {
    let mut bundle: Option<Vec<u8>> = None;
    let mut passphrase: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart parse failed: {e}")))?
    {
        match field.name().unwrap_or_default() {
            "bundle" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("bundle read failed: {e}")))?;
                bundle = Some(bytes.to_vec());
            }
            "passphrase" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("passphrase read failed: {e}")))?;
                passphrase = Some(text);
            }
            other => {
                tracing::debug!(
                    field = %other,
                    "ignoring unknown multipart field in preview_import: field={field}",
                    field = other,
                );
            }
        }
    }

    let bundle =
        bundle.ok_or_else(|| AppError::BadRequest("missing `bundle` multipart field".into()))?;
    let passphrase = passphrase
        .ok_or_else(|| AppError::BadRequest("missing `passphrase` multipart field".into()))?;
    Ok((bundle, passphrase))
}
