use axum::{
    Json,
    extract::{FromRequestParts, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum::http::request::Parts;
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::error::ErrorBody;
use crate::repository::Install;
use crate::state::AppState;

/// Maximum allowed clock skew between the Pi and the bridge (seconds).
const TIMESTAMP_WINDOW_SECS: i64 = 60;

/// Hard body-size limit applied to **every** incoming request, regardless of
/// whether it carries an `Authorization` header.
///
/// This is a DoS guard — it runs before any auth check so an attacker cannot
/// exhaust server memory by streaming a large body to an unauthenticated
/// endpoint. Authenticated endpoints are equally protected.
const MAX_BODY_BYTES: usize = 1024 * 1024; // 1 MiB

/// Path prefix for all authenticated install endpoints.
///
/// The auth middleware only attempts a DB token lookup when the request path
/// starts with this prefix. Requests to unauthenticated endpoints (health,
/// challenge, register, names) never incur a DB round-trip regardless of
/// whether they carry an `Authorization` header — this closes a DoS vector
/// where an attacker could force a DB query by sending a bearer token to
/// any endpoint.
const AUTHENTICATED_PATH_PREFIX: &str = "/v1/installs/";

// ── Axum extractor ───────────────────────────────────────────────────────────

/// Extractor that resolves to the install authenticated by the current request.
///
/// Reads the [`Install`] previously inserted into request extensions by
/// [`auth_layer`]. Returns `401 Unauthorized` if the extension is absent —
/// i.e. the request reached an authenticated handler without carrying a valid
/// `Authorization: Bearer` header.
pub struct AuthenticatedInstall(pub Install);

impl<S> FromRequestParts<S> for AuthenticatedInstall
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ErrorBody>);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Install>()
            .cloned()
            .map(AuthenticatedInstall)
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(ErrorBody { error: "authentication required".to_string() }),
                )
            })
    }
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Axum middleware that enforces a body-size limit on all requests and
/// authenticates those carrying an `Authorization: Bearer <token>` header
/// on install-owned endpoints.
///
/// # Body-size guard (unconditional)
///
/// Every request body is buffered up to [`MAX_BODY_BYTES`] before any other
/// processing. Requests that exceed the limit are rejected with `413 Payload
/// Too Large` immediately — before authentication, before routing, before any
/// handler code runs. This prevents memory exhaustion on unauthenticated
/// endpoints such as `POST /v1/register`.
///
/// # Authentication (only for `/v1/installs/*` paths)
///
/// The DB token lookup is only performed when:
/// 1. The request path starts with `/v1/installs/`, **and**
/// 2. An `Authorization: Bearer` header is present.
///
/// This avoids a DoS vector where an attacker sends a bearer token to an
/// unauthenticated endpoint (health, challenge, register, names) and forces a
/// DB query on every request.
///
/// When both conditions are met:
///
/// 1. Parse `Bearer <token>`, SHA-256 hash it, look up the install.
/// 2. Validate `X-Wardnet-Timestamp` — must be within ±60 s of now.
/// 3. Compute `canonical_payload = "<METHOD>\n<path_and_query>\n<ts>\n<hex-sha256(body)>"`.
/// 4. Verify the Ed25519 signature over the canonical payload using the
///    install's stored public key (decoded once at row load).
/// 5. Check the replay cache: reject if `(install_id, timestamp, body_hash)` was
///    already seen within the ±120 s replay window.
/// 6. On success, insert the [`Install`] into request extensions so
///    [`AuthenticatedInstall`] can retrieve it from any handler.
///
/// The buffered body is always reconstituted into the request so downstream
/// `Json<T>` extractors work normally.
pub async fn auth_layer(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let (mut parts, body) = request.into_parts();

    // ── Body-size guard (runs for ALL requests) ───────────────────────────
    let body_bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ErrorBody { error: "request body exceeds 1 MiB limit".to_string() }),
            )
                .into_response();
        }
    };

    // ── Auth (only for /v1/installs/* when Authorization header is present) ─
    let path = parts.uri.path();
    let auth_header = parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    if path.starts_with(AUTHENTICATED_PATH_PREFIX) {
        if let Some(auth_str) = auth_header {
            let Some(token) = auth_str.strip_prefix("Bearer ") else {
                return unauthorized("invalid Authorization header format");
            };
            let token = token.to_string();

            // Step 1: look up install by SHA-256(token).
            let token_hash = hex::encode(Sha256::digest(token.as_bytes()));
            let install = match state.installs().find_by_token_hash(&token_hash).await {
                Ok(Some(i)) => i,
                Ok(None) => return unauthorized("unknown bearer token"),
                Err(e) => {
                    tracing::error!(error = %e, "database error during auth");
                    return internal_error();
                }
            };

            // Step 2: validate timestamp.
            let timestamp_str = parts
                .headers
                .get("X-Wardnet-Timestamp")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            let timestamp: i64 = match timestamp_str.parse() {
                Ok(t) => t,
                Err(_) => return unauthorized("missing or invalid X-Wardnet-Timestamp"),
            };

            let now = chrono::Utc::now().timestamp();
            if (now - timestamp).abs() > TIMESTAMP_WINDOW_SECS {
                return unauthorized("X-Wardnet-Timestamp outside ±60 s window");
            }

            // Step 3: canonical payload — include path AND query string so
            // query parameters are covered by the signature.
            let method = parts.method.as_str();
            let path_and_query = parts
                .uri
                .path_and_query()
                .map(|pq| pq.as_str())
                .unwrap_or(path);
            let body_hash = hex::encode(Sha256::digest(&body_bytes));
            let payload =
                format!("{method}\n{path_and_query}\n{timestamp}\n{body_hash}");

            // Step 4: Ed25519 signature using the pre-decoded key bytes.
            let sig_b64 = parts
                .headers
                .get("X-Wardnet-Signature")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");

            if let Err(e) =
                verify_signature_bytes(&install.pub_key_bytes, payload.as_bytes(), sig_b64)
            {
                tracing::warn!(
                    install_id = %install.id,
                    error = %e,
                    "Ed25519 signature verification failed"
                );
                return unauthorized("invalid request signature");
            }

            // Step 5: replay check.
            let replay_key =
                format!("{}:{}:{}", install.id, timestamp, body_hash);
            if state.replay_cache().contains_or_insert(&replay_key, now) {
                tracing::warn!(
                    install_id = %install.id,
                    "replayed signed request rejected"
                );
                return unauthorized("replayed request");
            }

            // Step 6: stamp the verified install onto the request.
            parts.extensions.insert(install);
        }
    }

    // Reconstitute the request with the buffered body so downstream
    // extractors (`Json<T>`, `axum::body::Bytes`) see a normal body stream.
    let request = Request::from_parts(parts, axum::body::Body::from(body_bytes));
    next.run(request).await
}

// ── Signature verification ────────────────────────────────────────────────────

/// Verify an Ed25519 signature using the pre-decoded key bytes stored on
/// the [`Install`].
///
/// Avoids the base64 decode + allocation that the previous `verify_signature`
/// function performed on every authenticated request.
fn verify_signature_bytes(
    pub_key_bytes: &[u8; 32],
    message: &[u8],
    signature_b64: &str,
) -> anyhow::Result<()> {
    let verifying_key = VerifyingKey::from_bytes(pub_key_bytes)?;

    let sig_bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .map_err(|e| anyhow::anyhow!("base64-decode signature: {e}"))?;
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("Ed25519 signature must be exactly 64 bytes"))?;
    let signature = Signature::from_bytes(&sig_array);

    verifying_key.verify(message, &signature)?;
    Ok(())
}

// ── Error helpers ─────────────────────────────────────────────────────────────

fn unauthorized(msg: &str) -> Response {
    (StatusCode::UNAUTHORIZED, Json(ErrorBody { error: msg.to_string() })).into_response()
}

fn internal_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorBody { error: "internal server error".to_string() }),
    )
        .into_response()
}

// Full-stack auth middleware tests live in tests/api.rs.
// Signature verification round-trip tests require a signing key and the full
// request pipeline; those are authored in the integration test suite.
