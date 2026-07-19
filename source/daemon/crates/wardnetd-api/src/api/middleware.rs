use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::state::AppState;
use wardnetd_services::error::AppError;
use wardnetd_services::request_context::RequestId;

/// Extractor that resolves the client IP from the TCP connection.
///
/// Uses axum's `ConnectInfo` to get the peer socket address. This is the
/// real source IP on the LAN — no proxy headers needed for the MVP.
pub struct ClientIp(pub IpAddr);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let connect_info = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing ConnectInfo extension")))?;

        Ok(Self(connect_info.0.ip()))
    }
}

/// Per-listener marker inserted **only** on the TLS (`:443`) app by
/// `guarded_https_app`. Its presence tells cookie-issuing handlers
/// ([`auth::login`](crate::api::auth::login) / [`auth::refresh`](crate::api::auth::refresh))
/// to add the `Secure` attribute to the session cookie.
///
/// It is deliberately **absent** on the plain-HTTP `:7411` surface (the
/// pre-provisioning admin endpoint, which is the only reachable surface until a
/// real cert is issued via the DDNS/BYO-domain flow) and on the mock/dev server.
/// A browser silently drops a `Secure` cookie delivered over `http://`, so
/// marking those surfaces would strand the session cookie and break login there.
/// Fail direction is intentional: forgetting the marker yields a non-`Secure`
/// cookie (login works) rather than a silently-dropped one.
#[derive(Clone, Copy)]
pub struct SecureTransport;

/// Extractor that validates admin authentication.
///
/// Tries session cookie first, then `Authorization: Bearer <token>`
/// (interpreted as either a session token or an API key). Delegates all
/// cryptographic verification to [`AuthService`](wardnetd_services::AuthService) —
/// no SQL or hashing happens here.
pub struct AdminAuth {
    pub admin_id: Uuid,
    /// The raw session token that authenticated this request — the cookie or
    /// Bearer value that `validate_session` actually accepted, never a
    /// credential that merely rode along in the headers. `None` when the
    /// request authenticated via API key (no session exists to act on).
    /// Provided to downstream handlers (refresh, logout) so they operate on
    /// the same session that authorized them.
    pub session_token: Option<String>,
}

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;

        // Cookie first. If the cookie validates, it is the authenticating
        // session token.
        if let Some(token) = extract_cookie_token(headers)
            && let Some(admin_id) = state.auth_service().validate_session(&token).await?
        {
            return Ok(Self {
                admin_id,
                session_token: Some(token),
            });
        }

        // Then the Bearer value: a session token or an API key. The
        // distinction matters downstream — `session_token` must only carry a
        // value that names an actual session row, so refresh/logout act on
        // the session that authorized the request (a request can carry a
        // stale cookie alongside a valid bearer, and an API key is not a
        // session at all).
        if let Some(bearer) = extract_bearer_token(headers) {
            if let Some(admin_id) = state.auth_service().validate_session(&bearer).await? {
                return Ok(Self {
                    admin_id,
                    session_token: Some(bearer),
                });
            }
            if let Some(admin_id) = state.auth_service().validate_api_key(&bearer).await? {
                return Ok(Self {
                    admin_id,
                    session_token: None,
                });
            }
        }

        Err(AppError::Unauthorized(
            "valid session cookie or API key required".to_owned(),
        ))
    }
}

/// Extract the raw `wardnet_session` cookie value without validating it.
fn extract_cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.split(';').find_map(|pair| {
                let mut parts = pair.trim().splitn(2, '=');
                let name = parts.next()?.trim();
                let value = parts.next()?.trim();
                if name == "wardnet_session" && !value.is_empty() {
                    Some(value.to_owned())
                } else {
                    None
                }
            })
        })
}

/// Extract the raw `Authorization: Bearer` value without validating it.
fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

/// Extract and validate the `wardnet_session` cookie via the auth service.
async fn try_session_cookie(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<Option<Uuid>, AppError> {
    let Some(token) = extract_cookie_token(headers) else {
        return Ok(None);
    };

    state.auth_service().validate_session(&token).await
}

/// Extract and validate an `Authorization: Bearer <token>` header.
///
/// The login endpoint documents bearer-replay of the same token it sets in
/// the `wardnet_session` cookie (so non-browser callers — `wctl`, scripts,
/// integration tests — can authenticate without a cookie jar), and admins
/// can also mint long-lived API keys validated through a separate code path.
/// Try the session-token interpretation first, fall back to the API-key
/// path if that fails. Returns the authenticated admin's id, or `None` if
/// neither validator accepts the bearer.
async fn try_bearer(headers: &HeaderMap, state: &AppState) -> Result<Option<Uuid>, AppError> {
    let Some(bearer_token) = extract_bearer_token(headers) else {
        return Ok(None);
    };

    if let Some(id) = state.auth_service().validate_session(&bearer_token).await? {
        return Ok(Some(id));
    }

    state.auth_service().validate_api_key(&bearer_token).await
}

/// Axum middleware that resolves the [`AuthContext`] for every request.
///
/// If the request carries a valid admin session or API key the context is
/// [`AuthContext::Admin`]. Otherwise, the caller's IP is looked up in the
/// device repository to produce [`AuthContext::Device`] with the device's
/// MAC address. If neither succeeds, [`AuthContext::Anonymous`] is used.
///
/// The resolved context is inserted into the request extensions so that
/// [`AuthContextLayer`](wardnetd_services::auth_context::AuthContextLayer) can propagate
/// it into the `tokio::task_local` scope.
pub async fn resolve_auth_context(
    State(state): State<AppState>,
    mut req: axum::extract::Request,
    next: Next,
) -> Response {
    let headers = req.headers();

    // Try admin auth first (session cookie, then API key).
    let admin_id = try_session_cookie(headers, &state)
        .await
        .ok()
        .flatten()
        .or(try_bearer(headers, &state).await.ok().flatten());

    let ctx = if let Some(id) = admin_id {
        AuthContext::Admin { admin_id: id }
    } else {
        // Try to identify the caller by client IP -> device MAC.
        let ip = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip());

        if let Some(ip) = ip {
            match state
                .device_service()
                .get_device_for_ip(&ip.to_string())
                .await
            {
                Ok(resp) if resp.device.is_some() => AuthContext::Device {
                    mac: resp.device.unwrap().mac,
                },
                _ => AuthContext::Anonymous,
            }
        } else {
            AuthContext::Anonymous
        }
    };

    req.extensions_mut().insert(ctx);
    next.run(req).await
}

/// Axum middleware that generates a request ID, propagates correlation IDs,
/// and emits a W3C `traceparent` header on every response.
///
/// For each request this middleware:
/// 1. Generates a UUID v4 as the `X-Request-Id`.
/// 2. Reads `X-Correlation-Id` from the incoming headers (if present).
/// 3. Records both values in the current tracing span.
/// 4. Stores the request ID in request extensions as [`RequestId`] so the
///    [`RequestContextLayer`](wardnetd_services::request_context::RequestContextLayer)
///    can propagate it into the `tokio::task_local` scope.
/// 5. After the inner handler completes, sets response headers:
///    - `X-Request-Id`
///    - `X-Correlation-Id` (only if it was present on the request)
///    - `traceparent` per W3C Trace Context (version `00`, sampled)
pub async fn inject_request_context(mut req: axum::extract::Request, next: Next) -> Response {
    let request_id = Uuid::new_v4();
    let request_id_str = request_id.to_string();

    // Read optional correlation ID from incoming headers.
    let correlation_id = req
        .headers()
        .get("x-correlation-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    // Record in the current tracing span so they appear in structured logs.
    let span = tracing::Span::current();
    span.record("request_id", &request_id_str);
    if let Some(ref cid) = correlation_id {
        span.record("correlation_id", cid.as_str());
    }

    // Store in request extensions for the RequestContextLayer task-local.
    req.extensions_mut()
        .insert(RequestId(request_id_str.clone()));

    // Build the traceparent header (W3C Trace Context).
    // trace_id: UUID without hyphens (32 hex chars).
    // span_id:  first 16 hex chars of a new UUID (8 bytes = 16 hex chars).
    let trace_id = request_id.as_simple().to_string();
    let span_id = &Uuid::new_v4().as_simple().to_string()[..16];
    let traceparent = format!("00-{trace_id}-{span_id}-01");

    let mut response = next.run(req).await;

    // Inject response headers.
    let headers = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&request_id_str) {
        headers.insert("x-request-id", v);
    }
    if let Some(ref cid) = correlation_id
        && let Ok(v) = HeaderValue::from_str(cid)
    {
        headers.insert("x-correlation-id", v);
    }
    if let Ok(v) = HeaderValue::from_str(&traceparent) {
        headers.insert("traceparent", v);
    }

    response
}
