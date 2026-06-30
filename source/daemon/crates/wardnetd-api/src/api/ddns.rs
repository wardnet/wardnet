//! Dynamic-DNS HTTP handlers — `/api/ddns/...` (issues #527/#530).
//!
//! Drives the setup wizard's "Remote access (HTTPS)" step and (later) the
//! Settings surface: the wardnet enrollment flow (request code → enroll → check
//! slug → register network), BYOD-Cloudflare configuration, and the current DDNS
//! status. All endpoints are admin-gated — the wizard runs the operator as the
//! auto-logged-in admin from step 1 onward, so no pre-auth elevation is needed.
//!
//! Registration persists the provider identity **synchronously** (so the
//! response carries the assigned FQDN), then kicks a **detached** task to
//! publish the public A record and issue the certificate. That task is opaque
//! and slow (an ACME round-trip), so it must not block the wizard; progress is
//! polled via [`super::tls::tls_status`]. The task is idempotent, so a daemon
//! restart that orphans it is harmless — the 12h `TlsRenewalRunner` re-picks-up.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use tracing::Instrument;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use wardnet_common::api::{
    ConfigureCloudflareRequest, DdnsCheckResponse, DdnsEnrollRequest, DdnsEnrollmentCodeRequest,
    DdnsRegisterRequest, DdnsRegisterResponse, DdnsResolutionCheckResponse, DdnsStatusResponse,
};

use crate::api::middleware::AdminAuth;
use crate::api::responses::{AuthErrors, BadRequest};
use crate::state::AppState;
use wardnetd_services::error::AppError;

pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router
        .routes(routes!(ddns_enrollment_code))
        .routes(routes!(ddns_enroll))
        .routes(routes!(ddns_check))
        .routes(routes!(ddns_register))
        .routes(routes!(ddns_cloudflare))
        .routes(routes!(ddns_status))
        .routes(routes!(ddns_resolution_check))
        .routes(routes!(ddns_teardown))
}

/// Query string for `GET /api/ddns/check`.
#[derive(Deserialize)]
pub struct CheckQuery {
    slug: String,
}

/// Spawn the detached, admin-context provisioning task: publish the public A
/// record, then issue the certificate. Marks `Issuing` is the caller's job (so
/// a poll right after register already reflects it); this only runs the slow
/// work and logs the outcome — failures surface through the persisted TLS
/// provisioning phase, not this task's result.
fn spawn_provisioning(state: &AppState, admin_id: Uuid) {
    let ddns = state.ddns_service_arc();
    let tls = state.tls_service_arc();
    let ctx = AuthContext::Admin { admin_id };
    // Spawned tasks do not inherit the request's span, so attach our own child
    // span (rooted at the current request span) — see `.agents/observability.md`.
    let span = tracing::info_span!("remote_access_provisioning");
    tokio::spawn(
        async move {
            wardnetd_services::auth_context::with_context(ctx, async move {
                if let Err(e) = ddns.refresh_public_ip().await {
                    tracing::warn!(error = %e, "remote-access provisioning: public A-record publish failed: {e}");
                }
                match tls.ensure_certificate().await {
                    Ok(status) => {
                        tracing::info!(?status, "remote-access provisioning: certificate issuance finished");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "remote-access provisioning: certificate issuance failed: {e}");
                    }
                }
            })
            .await;
        }
        .instrument(span),
    );
}

#[utoipa::path(
    post,
    path = "/api/ddns/enrollment-code",
    tag = "ddns",
    description = "Request a one-time enrollment code be emailed to the wardnet \
                   account. Step 1 of the wardnet remote-access flow; the operator \
                   then submits the emailed code to `POST /api/ddns/enroll`. \
                   Admin only.",
    request_body = DdnsEnrollmentCodeRequest,
    responses(
        (status = 204, description = "Code requested (emailed if the account exists)"),
        AuthErrors,
        BadRequest,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn ddns_enrollment_code(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<DdnsEnrollmentCodeRequest>,
) -> Result<StatusCode, AppError> {
    state
        .ddns_service()
        .request_enrollment_code(body.email)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/ddns/enroll",
    tag = "ddns",
    description = "Enroll this daemon against the one-time code: generate its cloud \
                   identity and bind it to the tenant. Step 2 of the wardnet flow; \
                   afterwards check slug availability and register a network. \
                   Admin only.",
    request_body = DdnsEnrollRequest,
    responses(
        (status = 204, description = "Enrolled"),
        AuthErrors,
        BadRequest,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn ddns_enroll(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(body): Json<DdnsEnrollRequest>,
) -> Result<StatusCode, AppError> {
    state.ddns_service().enroll(body.code).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/ddns/check",
    tag = "ddns",
    description = "Check whether a vanity slug is available. Returns \
                   `available: false` for malformed or reserved slugs too (no \
                   error). Requires a prior enroll. Admin only.",
    params(("slug" = String, Query, description = "The vanity slug to check, e.g. happy-einstein")),
    responses(
        (status = 200, description = "Availability result", body = DdnsCheckResponse),
        AuthErrors,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn ddns_check(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Query(query): Query<CheckQuery>,
) -> Result<Json<DdnsCheckResponse>, AppError> {
    let available = state.ddns_service().check_slug(query.slug).await?;
    Ok(Json(DdnsCheckResponse { available }))
}

#[utoipa::path(
    post,
    path = "/api/ddns/register",
    tag = "ddns",
    description = "Register a network under the given vanity slug on the \
                   lowest-latency region, then kick off certificate issuance in \
                   the background. Requires a prior enroll. The response returns as \
                   soon as the identity is persisted; poll `GET /api/tls/status` \
                   for issuance progress. Admin only.",
    request_body = DdnsRegisterRequest,
    responses(
        (status = 200, description = "Registered; provisioning started", body = DdnsRegisterResponse),
        AuthErrors,
        BadRequest,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn ddns_register(
    State(state): State<AppState>,
    auth: AdminAuth,
    Json(body): Json<DdnsRegisterRequest>,
) -> Result<Json<DdnsRegisterResponse>, AppError> {
    let registration = state
        .ddns_service()
        .register_network(body.slug, body.display_name)
        .await?;
    // Reflect "issuing" before the spawn runs, so a poll racing the task still
    // shows progress rather than a stale idle/failed phase.
    state.tls_service().mark_provisioning_started().await?;
    spawn_provisioning(&state, auth.admin_id);
    Ok(Json(DdnsRegisterResponse {
        fqdn: registration.subdomain,
        region: Some(registration.region),
    }))
}

#[utoipa::path(
    post,
    path = "/api/ddns/cloudflare",
    tag = "ddns",
    description = "Configure the BYOD-Cloudflare provider for a domain the \
                   operator controls (token validated against the zone), then \
                   kick off certificate issuance in the background. Poll \
                   `GET /api/tls/status` for progress. Admin only.",
    request_body = ConfigureCloudflareRequest,
    responses(
        (status = 200, description = "Configured; provisioning started", body = DdnsRegisterResponse),
        AuthErrors,
        BadRequest,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn ddns_cloudflare(
    State(state): State<AppState>,
    auth: AdminAuth,
    Json(body): Json<ConfigureCloudflareRequest>,
) -> Result<Json<DdnsRegisterResponse>, AppError> {
    let registration = state
        .ddns_service()
        .configure_cloudflare(body.token, body.domain)
        .await?;
    state.tls_service().mark_provisioning_started().await?;
    spawn_provisioning(&state, auth.admin_id);
    Ok(Json(DdnsRegisterResponse {
        fqdn: registration.subdomain,
        // BYOD has no bridge region.
        region: None,
    }))
}

#[utoipa::path(
    get,
    path = "/api/ddns/status",
    tag = "ddns",
    description = "Report the current DDNS configuration (provider, active \
                   hostname, last-published IP). Admin only.",
    responses(
        (status = 200, description = "DDNS status", body = DdnsStatusResponse),
        AuthErrors,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn ddns_status(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<DdnsStatusResponse>, AppError> {
    let status = state.ddns_service().status().await?;
    Ok(Json(DdnsStatusResponse {
        provider: status.provider,
        fqdn: status.fqdn,
        last_public_ip: status.last_public_ip,
        suspended: status.suspended,
    }))
}

#[utoipa::path(
    get,
    path = "/api/ddns/resolution-check",
    tag = "ddns",
    description = "Check whether the public internet resolves the active FQDN to \
                   the IP the daemon last published. Queries fixed external \
                   resolvers (1.1.1.1 / 8.8.8.8) by IP, deliberately bypassing the \
                   local split-horizon override. `pending` means no public A \
                   record yet (normal right after registration). Admin only.",
    responses(
        (status = 200, description = "Resolution-check result", body = DdnsResolutionCheckResponse),
        AuthErrors,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn ddns_resolution_check(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<Json<DdnsResolutionCheckResponse>, AppError> {
    let result = state.ddns_service().resolution_check().await?;
    Ok(Json(result))
}

#[utoipa::path(
    delete,
    path = "/api/ddns",
    tag = "ddns",
    description = "Disable remote access: remove this daemon from its network \
                   (wardnet) or delete the Cloudflare record (BYOD), drop the \
                   stored certificate, and revert :443 to the unprovisioned \
                   placeholder. Idempotent. Admin only.",
    responses(
        (status = 204, description = "Remote access torn down (or already absent)"),
        AuthErrors,
    ),
    security(("session_cookie" = []), ("bearer_auth" = [])),
)]
pub async fn ddns_teardown(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> Result<StatusCode, AppError> {
    // Revert serving + drop the cert first, then release the DDNS identity. The
    // two services own disjoint state, so order only affects the brief window
    // between the cert going away and the public record being removed — closing
    // :443 first is the cleaner shutdown sequence.
    state.tls_service().teardown().await?;
    state.ddns_service().teardown().await?;
    Ok(StatusCode::NO_CONTENT)
}
