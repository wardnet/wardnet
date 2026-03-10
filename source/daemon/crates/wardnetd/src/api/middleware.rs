use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;
use wardnet_types::auth::AuthContext;

use crate::error::AppError;
use crate::request_context::RequestId;
use crate::state::AppState;

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

/// Extractor that validates admin authentication.
///
/// Tries session cookie first, then Bearer API key. Delegates all
/// cryptographic verification to [`AuthService`](crate::service::AuthService) —
/// no SQL or hashing happens here.
pub struct AdminAuth {
    pub admin_id: Uuid,
}

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;

        if let Some(admin_id) = try_session_cookie(headers, state).await? {
            return Ok(Self { admin_id });
        }

        if let Some(admin_id) = try_api_key(headers, state).await? {
            return Ok(Self { admin_id });
        }

        Err(AppError::Unauthorized(
            "valid session cookie or API key required".to_owned(),
        ))
    }
}

/// Extract and validate the `wardnet_session` cookie via the auth service.
async fn try_session_cookie(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<Option<Uuid>, AppError> {
    let cookie_header = match headers.get(axum::http::header::COOKIE) {
        Some(v) => v.to_str().unwrap_or_default(),
        None => return Ok(None),
    };

    let token = cookie_header.split(';').find_map(|pair| {
        let mut parts = pair.trim().splitn(2, '=');
        let name = parts.next()?.trim();
        let value = parts.next()?.trim();
        if name == "wardnet_session" {
            Some(value.to_owned())
        } else {
            None
        }
    });

    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => return Ok(None),
    };

    state.auth_service().validate_session(&token).await
}

/// Extract and validate the `Authorization: Bearer <key>` header via the auth service.
async fn try_api_key(headers: &HeaderMap, state: &AppState) -> Result<Option<Uuid>, AppError> {
    let auth_header = match headers.get(axum::http::header::AUTHORIZATION) {
        Some(v) => v.to_str().unwrap_or_default(),
        None => return Ok(None),
    };

    let bearer_token = match auth_header.strip_prefix("Bearer ") {
        Some(t) if !t.is_empty() => t,
        _ => return Ok(None),
    };

    state.auth_service().validate_api_key(bearer_token).await
}

/// Axum middleware that resolves the [`AuthContext`] for every request.
///
/// If the request carries a valid admin session or API key the context is
/// [`AuthContext::Admin`]. Otherwise, the caller's IP is looked up in the
/// device repository to produce [`AuthContext::Device`] with the device's
/// MAC address. If neither succeeds, [`AuthContext::Anonymous`] is used.
///
/// The resolved context is inserted into the request extensions so that
/// [`AuthContextLayer`](crate::auth_context::AuthContextLayer) can propagate
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
        .or(try_api_key(headers, &state).await.ok().flatten());

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
///    [`RequestContextLayer`](crate::request_context::RequestContextLayer)
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
