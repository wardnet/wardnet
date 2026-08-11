//! TLS provisioning HTTP handler — `/api/tls/status` (issues #528/#530).
//!
//! Read-only window onto the daemon-owned certificate's coarse provisioning
//! phase (`idle`/`issuing`/`issued`/`failed`) plus the active domain, expiry,
//! and last error. The setup wizard polls this after kicking registration, and
//! the dashboard's persistent indicator uses it to show provisioning progress
//! after setup completes. Admin only; never touches the ACME server.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use wardnet_common::api::TlsStatusResponse;

use crate::api::middleware::SessionAuth;
use crate::api::responses::AuthErrors;
use crate::state::AppState;
use wardnetd_services::error::AppError;

pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(tls_status))
}

#[utoipa::path(
    get,
    path = "/api/tls/status",
    tag = "tls",
    description = "Report the certificate's coarse provisioning phase \
                   (idle/issuing/issued/failed) with the active domain, expiry, \
                   and last error. Admin only.",
    responses(
        (status = 200, description = "TLS provisioning status", body = TlsStatusResponse),
        AuthErrors,
    ),
)]
pub async fn tls_status(
    State(state): State<AppState>,
    _auth: SessionAuth,
) -> Result<Json<TlsStatusResponse>, AppError> {
    Ok(Json(state.tls_service().provisioning_status().await?))
}
