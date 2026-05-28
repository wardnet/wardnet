use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::HeaderMap;
use chrono::Utc;
use serde::Serialize;
use std::net::SocketAddr;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::ApiError;
use crate::repository::RegistrationChallenge;
use crate::state::AppState;

/// `PoW` difficulty: number of leading zero bits required in
/// `SHA256(nonce\nname\npublic_key\nproof)`.
///
/// 24 bits → ~16 M expected hashes → ~160 ms on a Pi 4 (acceptable for a
/// one-time setup step), ~4 h to register all 900 word-pair names even on a
/// fast laptop, longer still on a typical residential IP limited by the
/// registration rate cap.
pub const POW_DIFFICULTY: u32 = 24;

/// Challenge lifetime. After this the nonce is expired and the client must
/// fetch a new one.
const CHALLENGE_EXPIRY_SECS: i64 = 300; // 5 minutes

/// Maximum challenges issued per remote IP per hour.
///
/// 20 is generous enough for legitimate retries (e.g. name conflicts during
/// wizard flow) while capping any attempt to pre-compute a challenge pool.
const CHALLENGES_PER_IP_PER_HOUR: i64 = 20;

/// Register the challenge route.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(get_challenge))
}

/// Response body for `GET /v1/register/challenge`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ChallengeResponse {
    /// Opaque challenge UUID. Pass this as `challenge_id` in
    /// `POST /v1/register`.
    pub challenge_id: String,
    /// 32 random bytes as lowercase hex. Include verbatim in the `PoW` input.
    pub nonce: String,
    /// Number of leading zero bits the `SHA256` output must have.
    pub difficulty: u32,
    /// ISO 8601 UTC timestamp after which the challenge is invalid.
    pub expires_at: String,
}

#[utoipa::path(
    get,
    path = "/v1/register/challenge",
    tag = "installs",
    description = "Issue a single-use proof-of-work challenge that must be solved before \
                   calling `POST /v1/register`. \
                   \n\n\
                   The client must find a `proof` (u64) such that \
                   `SHA256(nonce\\nname\\npublic_key\\nproof_decimal)` has at least \
                   `difficulty` leading zero bits. \
                   \n\n\
                   Challenges expire after 5 minutes and are burned on first use. \
                   Rate-limited to 20 requests per remote IP per hour.",
    responses(
        (status = 200, description = "PoW challenge issued", body = ChallengeResponse),
        (status = 429, description = "Challenge rate limit exceeded (20/IP/hour)"),
        (status = 500, description = "Internal server error"),
    ),
    security(()),
)]
pub async fn get_challenge(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<ChallengeResponse>, ApiError> {
    let remote_ip = client_ip(&headers, addr);

    // ── Rate limit: 20 challenges per IP per hour ─────────────────────────
    let since = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let count = state
        .challenges()
        .count_from_ip(&remote_ip, &since)
        .await
        .map_err(ApiError::Internal)?;

    if count >= CHALLENGES_PER_IP_PER_HOUR {
        return Err(ApiError::TooManyRequests(
            "challenge rate limit exceeded (20 per IP per hour)".to_string(),
        ));
    }

    // ── Generate and persist challenge ────────────────────────────────────
    let mut nonce_bytes = [0u8; 32];
    rand::fill(&mut nonce_bytes);
    let nonce = hex::encode(nonce_bytes);

    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(CHALLENGE_EXPIRY_SECS);

    let challenge = RegistrationChallenge {
        id: Uuid::new_v4().to_string(),
        nonce: nonce.clone(),
        difficulty: POW_DIFFICULTY,
        remote_ip,
        created_at: now,
        expires_at,
        used_at: None,
    };

    state
        .challenges()
        .insert(&challenge)
        .await
        .map_err(ApiError::Internal)?;

    Ok(Json(ChallengeResponse {
        challenge_id: challenge.id,
        nonce,
        difficulty: POW_DIFFICULTY,
        expires_at: expires_at.to_rfc3339(),
    }))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract the real client IP from the request.
///
/// The `X-Forwarded-For` header is only trusted when the TCP peer address is a
/// loopback address (`127.x.x.x` or `::1`). In production the bridge always
/// sits behind Caddy on the same host, so the peer is `127.0.0.1` and Caddy's
/// `X-Forwarded-For` header carries the real client IP.
///
/// When the peer is not loopback (direct connection, development, tests), the
/// TCP peer address is used as-is — trusting a client-supplied
/// `X-Forwarded-For` would allow IP spoofing for the rate-limit and challenge
/// binding checks.
#[must_use]
pub fn client_ip(headers: &HeaderMap, addr: SocketAddr) -> String {
    if addr.ip().is_loopback()
        && let Some(forwarded_for) = headers
            .get("X-Forwarded-For")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
            .map(str::trim)
    {
        return forwarded_for.to_string();
    }
    addr.ip().to_string()
}

/// Verify a proof-of-work solution.
///
/// Returns `true` when
/// `SHA256(nonce\nname\npublic_key\nproof_decimal).leading_zeros() >= difficulty`.
///
/// The canonical payload uses `\n` separators — the same convention as the
/// request-signing scheme — so the derivation is unambiguous regardless of
/// field lengths.
#[must_use]
pub fn verify_pow(nonce: &str, name: &str, public_key: &str, proof: u64, difficulty: u32) -> bool {
    use sha2::{Digest, Sha256};
    let payload = format!("{nonce}\n{name}\n{public_key}\n{proof}");
    let hash = Sha256::digest(payload.as_bytes());

    let mut bits = 0u32;
    for byte in &hash {
        let z = byte.leading_zeros();
        bits += z;
        if z < 8 {
            break;
        }
    }
    bits >= difficulty
}

#[cfg(test)]
mod tests;
