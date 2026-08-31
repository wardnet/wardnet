use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use uuid::Uuid;
use wardnet_common::auth::{AuthContext, AuthenticatedUser, UserRole};

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

    fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        // Reading an extension awaits nothing, so resolve it here and hand back
        // an already-complete future rather than building a state machine that
        // finishes on its first poll.
        std::future::ready(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing ConnectInfo extension")))
                .map(|connect_info| Self(connect_info.0.ip())),
        )
    }
}

/// Lets a handler ask for `Option<ClientIp>` and get `None` instead of a `500`
/// when `ConnectInfo` is absent.
///
/// `ConnectInfo` is missing on surfaces that were not built with
/// `into_make_service_with_connect_info` — the mock/dev server and several
/// handler tests. The login path wants the source address for per-IP rate
/// limiting but must not *require* it: refusing to log anyone in because a test
/// harness did not install `ConnectInfo` would be a far worse failure than
/// degrading to identity-only throttling.
impl axum::extract::OptionalFromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> impl std::future::Future<Output = Result<Option<Self>, Self::Rejection>> + Send {
        std::future::ready(Ok(parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| Self(ci.0.ip()))))
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

/// Extractor that validates that the caller holds a household-user session.
///
/// Tries session cookie first, then `Authorization: Bearer <token>`
/// (interpreted as either a session token or an API key). Delegates all
/// cryptographic verification to [`AuthService`](wardnetd_services::AuthService) —
/// no SQL or hashing happens here.
///
/// # Authentication, not authorization
///
/// Named `SessionAuth` rather than the former `SessionAuth` because it no longer
/// implies anything about *role*: a `member` satisfies it exactly as an `admin`
/// does.
/// Authorization is the service layer's job, via
/// `auth_context::require_admin()` (see `.agents/auth.md`). The old name
/// promised a check this extractor never performed, which with only one
/// principal was harmless and with two would be an escalation waiting for a
/// reader to trust the type name.
pub struct SessionAuth {
    /// The authenticated user, carrying their live role.
    pub user: AuthenticatedUser,
    /// The raw session token that authenticated this request — the cookie or
    /// Bearer value that `validate_session` actually accepted, never a
    /// credential that merely rode along in the headers. `None` when the
    /// request authenticated via API key (no session exists to act on).
    /// Provided to downstream handlers (refresh, logout) so they operate on
    /// the same session that authorized them.
    pub session_token: Option<String>,
}

impl FromRequestParts<AppState> for SessionAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;

        // Cookie first. If the cookie validates, it is the authenticating
        // session token.
        if let Some(token) = extract_cookie_token(headers)
            && let Some(user) = state.auth_service().validate_session(&token).await?
        {
            return Ok(Self {
                user,
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
            if let Some(user) = state.auth_service().validate_session(&bearer).await? {
                return Ok(Self {
                    user,
                    session_token: Some(bearer),
                });
            }
            if let Some(user) = state.auth_service().validate_api_key(&bearer).await? {
                return Ok(Self {
                    user,
                    session_token: None,
                });
            }
        }

        Err(AppError::Unauthorized(
            "valid session cookie or API key required".to_owned(),
        ))
    }
}

/// Extractor that requires an authenticated household user with
/// `role = Admin`.
///
/// # When to use this instead of [`SessionAuth`]
///
/// Almost never. The rule is that authorization lives in the service layer, so
/// a handler takes `SessionAuth` and the service it calls opens with
/// `auth_context::require_admin()`.
///
/// This exists for the handful of routes that have **no service call to put a
/// guard in** — the `OpenAPI` spec endpoint and the Scalar docs shell, which are
/// closures over embedded assets. Before household users those were gated by
/// `AdminAuth` being the only session extractor, so renaming it to `SessionAuth`
/// silently opened them to `member` callers. `/api/openapi.json` in particular
/// hands back the entire admin API surface, which is exactly the reconnaissance
/// material the gate existed to withhold.
///
/// If you are reaching for this on a route that *does* call a service, put
/// `require_admin()` in the service instead — that is where the truth table in
/// `wardnetd-services/src/tests/auth_context.rs` can see it.
pub struct AdminOnly;

impl FromRequestParts<AppState> for AdminOnly {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = SessionAuth::from_request_parts(parts, state).await?;

        // `403`, not `401`: the caller proved who they are, and the answer is
        // still no. Reporting 401 would invite a client to re-authenticate as
        // if the credential were the problem.
        if session.user.role() != UserRole::Admin {
            return Err(AppError::Forbidden("admin privileges required".to_owned()));
        }

        Ok(Self)
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
) -> Result<Option<AuthenticatedUser>, AppError> {
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
/// path if that fails. Returns the authenticated user, or `None` if neither
/// validator accepts the bearer.
async fn try_bearer(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<Option<AuthenticatedUser>, AppError> {
    let Some(bearer_token) = extract_bearer_token(headers) else {
        return Ok(None);
    };

    if let Some(user) = state.auth_service().validate_session(&bearer_token).await? {
        return Ok(Some(user));
    }

    state.auth_service().validate_api_key(&bearer_token).await
}

/// Axum middleware that resolves the [`AuthContext`] for every request.
///
/// If the request carries a valid household-user session or API key the context
/// is [`AuthContext::User`], carrying the role the credential actually resolved
/// to. Otherwise, the caller's IP is looked up in the device repository to
/// produce [`AuthContext::Device`] with the device's MAC address. If neither
/// succeeds, [`AuthContext::Anonymous`] is used.
///
/// Session/API-key resolution is tried **before** device-by-IP on purpose: a
/// signed-in person using a known household device must be attributed to
/// themselves, not demoted to the device principal they happen to be sitting at.
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

    // Try session auth first (session cookie, then bearer/API key).
    let authenticated = match try_session_cookie(headers, &state).await {
        Ok(Some(user)) => Some(user),
        // A failed cookie must not skip the bearer path: `wctl` and integration
        // tests replay the token as a bearer, and a browser can carry a stale
        // cookie alongside a valid one.
        _ => try_bearer(headers, &state).await.ok().flatten(),
    };

    // The principal is used exactly as the credential resolved it. Constructing
    // an `AuthenticatedUser` here instead — with a role this layer chose — is
    // the escalation ADR-0031 §2 exists to prevent: it is what promoted every
    // valid session to admin before household users existed.
    let ctx = if let Some(user) = authenticated {
        AuthContext::user(user)
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
