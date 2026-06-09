use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::api::challenge::client_ip;
use crate::api::validation::{validate_name, validate_public_key};
use crate::error::ApiError;
use crate::repository::{Install, RegistrationChallenge};
use crate::state::AppState;

/// Maximum registrations from the same remote IP per 24 hours.
///
/// A legitimate Pi registers exactly once during initial setup.
/// 3 allows for one retry if the first name is taken and one more in case of
/// a transient error, without handing a meaningful budget to an attacker who
/// has already exhausted their `PoW` challenges.
const REGISTRATIONS_PER_IP_PER_DAY: i64 = 3;

/// Register the `POST /v1/register` route.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(register_install))
}

/// Request body for `POST /v1/register`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    /// Desired subdomain slug, e.g. `"happy-einstein"`.
    /// Must match `[a-z0-9-]`, 3–32 characters, no leading/trailing hyphen.
    pub name: String,
    /// Base64-encoded raw Ed25519 verifying-key bytes (exactly 32 bytes).
    pub public_key: String,
    /// Challenge UUID obtained from `GET /v1/register/challenge`.
    pub challenge_id: String,
    /// `PoW` proof: a `u64` such that
    /// `SHA256(nonce\nname\npublic_key\nproof_decimal)` has at least
    /// `difficulty` leading zero bits.
    pub proof: u64,
}

/// Response body for `POST /v1/register`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RegisterResponse {
    /// Server-assigned installation UUID. Used in all subsequent API paths.
    pub id: String,
    /// Opaque bearer token. Store in the Pi's `SecretStore`.
    /// The bridge stores only `SHA256(token)` — this is the only time the
    /// raw value is returned.
    pub bearer_token: String,
    /// Fully-qualified subdomain assigned to this installation,
    /// e.g. `"happy-einstein.my.wardnet.services"`.
    pub subdomain: String,
    /// Region this bridge instance serves, e.g. `"us"` or `"eu"`.
    pub region: String,
}

#[utoipa::path(
    post,
    path = "/v1/register",
    tag = "installs",
    description = "Register a new wardnet installation. \
                   \n\n\
                   Requires a valid, unexpired PoW challenge obtained from \
                   `GET /v1/register/challenge`. The challenge is single-use and \
                   burned atomically on success. \
                   \n\n\
                   Rate-limited to **3 registrations per remote IP per 24 hours** \
                   (a legitimate Pi registers once at setup).",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "Installation registered", body = RegisterResponse),
        (status = 400, description = "Invalid name, public key, or PoW proof"),
        (status = 409, description = "Name already taken"),
        (status = 429, description = "Registration rate limit exceeded (3/IP/24 h)"),
        (status = 500, description = "Internal server error"),
    ),
    security(()),
)]
pub async fn register_install(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), ApiError> {
    let remote_ip = client_ip(&headers, addr);

    validate_name(&body.name)?;
    validate_public_key(&body.public_key)?;

    check_registration_rate_limit(&state, &remote_ip).await?;
    validate_challenge(&state, &body, &remote_ip).await?;

    // ── Generate install ID and bearer token ──────────────────────────────
    // The ID is needed for the global reservation (it links the names row to the
    // regional install row), so it is generated up front.
    let id = Uuid::new_v4().to_string();
    let (bearer_token, token_hash) = generate_token();
    let now = Utc::now();
    let region = state.config().region.clone();

    // ── Phase 1: RESERVE on the global naming authority ────────────────────
    // The slug PRIMARY KEY is the cross-region allocation lock: a unique
    // violation means the name is taken. This is the atomic uniqueness guard
    // (it replaces the old per-region `find_by_name` pre-check). It runs BEFORE
    // burning the challenge so a name clash never consumes the client's PoW.
    let reserved = state
        .names()
        .reserve(&body.name, &id, &region, now, now + RESERVATION_TTL)
        .await
        .map_err(ApiError::Internal)?;

    if !reserved {
        return Err(ApiError::Conflict(format!(
            "name '{}' is already taken",
            body.name
        )));
    }

    // From here on the registration is a two-database saga: any failure must
    // release the reservation AND drop the (possibly inserted) regional install
    // row, so a crash never leaks a name or strands an orphan.

    // ── Atomically burn the challenge (prevents replay) ───────────────────
    let consumed = match state.challenges().consume(&body.challenge_id, now).await {
        Ok(consumed) => consumed,
        Err(e) => {
            release(&state, &body.name, &id).await;
            return Err(ApiError::Internal(e));
        }
    };

    if !consumed {
        release(&state, &body.name, &id).await;
        return Err(ApiError::BadRequest(
            "challenge has already been used".to_string(),
        ));
    }

    // ── Phase 2: PROVISION the regional install row ───────────────────────
    // No DNS record is created here — the bridge is pure SNI passthrough; the
    // wildcard DNS is infra-provisioned and the per-user cert is daemon-issued.
    let pk_bytes = decode_public_key(&body.public_key);
    let install = Install {
        id: id.clone(),
        name: body.name.clone(),
        public_key: body.public_key.clone(),
        pub_key_bytes: pk_bytes,
        token_hash,
        ip: None,
        cf_a_record_id: None,
        cf_acme_record_ids: Vec::new(),
        created_at: now,
        updated_at: now,
    };

    if let Err(e) = state.installs().insert(&install).await {
        release(&state, &body.name, &id).await;
        return Err(ApiError::Internal(e));
    }

    // ── Phase 3: CONFIRM — promote the reservation to active ──────────────
    // Last step that can still fail the registration: only now is the name
    // immune to the sweep.
    if let Err(e) = state.names().confirm(&body.name).await {
        release(&state, &body.name, &id).await;
        return Err(ApiError::Internal(e));
    }

    // Record registration for rate-limit accounting. Best-effort: the
    // registration is already complete and confirmed, so a failure here (an
    // advisory counter) must not tear down a valid registration — log and
    // continue rather than compensating.
    if let Err(e) = state.installs().log_registration(&remote_ip, now).await {
        tracing::error!(install_id = %id, error = %e, "failed to log registration for rate-limiting");
    }

    tracing::info!(
        install_id = %id,
        name = %body.name,
        region = %region,
        "new installation registered"
    );

    Ok((
        StatusCode::CREATED,
        Json(RegisterResponse {
            id,
            bearer_token,
            subdomain: state.config().install_fqdn(&body.name),
            region,
        }),
    ))
}

/// Reservation TTL — how long a `reserved` (unconfirmed) name is held before the
/// sweep may reap it. Matches the `PoW` challenge TTL; provision is a fast DB
/// insert, so the only real risk window is a crash between reserve and confirm.
const RESERVATION_TTL: chrono::Duration = chrono::Duration::minutes(5);

/// Compensating action for the registration saga: release the global name
/// reservation and, **only if a reserved row was actually removed**, drop the
/// paired regional install row. Pairing the install delete to the name delete
/// keeps the two databases consistent even in the narrow window where a
/// `confirm` committed but its ack was lost (the name is `active`, so `release`
/// removes nothing and the install row is left in place). Best-effort: failures
/// are logged, not propagated (the caller is already returning an error), and a
/// release that can't run is recovered by the region-scoped sweep.
async fn release(state: &AppState, slug: &str, install_id: &str) {
    match state.names().release(slug).await {
        Ok(true) => {
            if let Err(e) = state.installs().delete(install_id).await {
                tracing::error!(
                    install_id = %install_id,
                    error = %e,
                    "failed to delete install row during reservation release"
                );
            }
        }
        // Nothing reserved to release (already active or gone) — leave the
        // install row paired with the surviving name; do not orphan-delete it.
        Ok(false) => {}
        Err(e) => {
            // The sweep will reap the reservation (and its install) later.
            tracing::error!(slug = %slug, error = %e, "failed to release name reservation");
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Enforce the per-IP registration rate limit (3 per IP per 24 h).
async fn check_registration_rate_limit(state: &AppState, remote_ip: &str) -> Result<(), ApiError> {
    let since_24h = Utc::now() - chrono::Duration::days(1);
    let reg_count = state
        .installs()
        .count_registrations_from_ip(remote_ip, since_24h)
        .await
        .map_err(ApiError::Internal)?;

    if reg_count >= REGISTRATIONS_PER_IP_PER_DAY {
        return Err(ApiError::TooManyRequests(
            "registration rate limit exceeded (3 per IP per 24 h)".to_string(),
        ));
    }
    Ok(())
}

/// Resolve the `PoW` challenge and verify expiry, IP binding, and proof.
///
/// Does **not** consume the challenge — that happens after the uniqueness check.
async fn validate_challenge(
    state: &AppState,
    body: &RegisterRequest,
    remote_ip: &str,
) -> Result<RegistrationChallenge, ApiError> {
    let challenge = state
        .challenges()
        .find_by_id(&body.challenge_id)
        .await
        .map_err(ApiError::Internal)?
        .ok_or_else(|| ApiError::BadRequest("unknown challenge_id".to_string()))?;

    if Utc::now() > challenge.expires_at {
        return Err(ApiError::BadRequest(
            "challenge has expired — fetch a new one from GET /v1/register/challenge".to_string(),
        ));
    }

    if challenge.remote_ip != remote_ip {
        return Err(ApiError::BadRequest(
            "challenge was issued to a different IP address".to_string(),
        ));
    }

    if !crate::api::challenge::verify_pow(
        &challenge.nonce,
        &body.name,
        &body.public_key,
        body.proof,
        challenge.difficulty,
    ) {
        return Err(ApiError::BadRequest(
            "proof-of-work verification failed".to_string(),
        ));
    }

    Ok(challenge)
}

/// Decode a validated base64 public key into raw bytes.
///
/// # Panics
/// Panics if `public_key` is not valid base64 or not exactly 32 bytes.
/// This should never happen because `validate_public_key` is called first.
fn decode_public_key(public_key: &str) -> [u8; 32] {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(public_key)
        .expect("public_key is valid base64 — validated above");
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    arr
}

/// Generate a random 32-byte bearer token.
///
/// Returns `(raw_token_hex, sha256_hex)`. Only the hash is stored; the raw
/// token is returned to the caller exactly once.
fn generate_token() -> (String, String) {
    use sha2::{Digest, Sha256};
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    let token = hex::encode(bytes);
    let hash = hex::encode(Sha256::digest(token.as_bytes()));
    (token, hash)
}

#[cfg(test)]
mod tests;
