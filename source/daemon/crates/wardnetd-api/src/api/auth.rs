use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::SET_COOKIE;
use axum::response::IntoResponse;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{ApiError, LoginRequest, LoginResponse};

use crate::api::middleware::AdminAuth;
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register auth routes onto the given [`OpenApiRouter`]. Each module owns its
/// own route list so `api::mod::router` stays a simple composition point.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(login)).routes(routes!(refresh))
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    description = "Log in with username and password. On success, sets a \
                   `wardnet_session` cookie the browser replays on subsequent \
                   admin-gated requests. The JSON body also carries the raw \
                   `token` and `expires_in_seconds` so non-browser clients \
                   (scripts, the `wctl` CLI, third-party integrations) can \
                   replay the token via `Authorization: Bearer <token>` \
                   without a cookie jar.",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful; session cookie is set", body = LoginResponse),
        (status = 400, description = "Malformed request body", body = ApiError),
        (status = 401, description = "Invalid credentials", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(()),
)]
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .auth_service()
        .login(&body.username, &body.password, body.remember_me)
        .await?;

    let cookie_value = format!(
        "wardnet_session={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        result.token, result.max_age_seconds
    );

    Ok((
        [(SET_COOKIE, cookie_value)],
        Json(LoginResponse {
            message: "logged in".to_owned(),
            token: result.token,
            expires_in_seconds: result.max_age_seconds,
        }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/auth/refresh",
    tag = "auth",
    description = "Slide the current session's expiry forward by 30 days. \
                   Called by the admin-app on every open to implement the \
                   'login once' sliding-window behaviour. Requires a valid \
                   session cookie or `Authorization: Bearer <token>` header.",
    responses(
        (status = 204, description = "Session expiry extended; fresh Set-Cookie issued"),
        (status = 401, description = "No valid session", body = ApiError),
        (status = 403, description = "Session was not created with remember_me", body = ApiError),
        (status = 500, description = "Internal server error", body = ApiError),
    ),
    security(
        ("session_cookie" = []),
        ("bearer_auth" = []),
    ),
)]
pub async fn refresh(
    State(state): State<AppState>,
    _auth: AdminAuth,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let token = extract_session_token(&headers)
        .ok_or_else(|| AppError::Unauthorized("no session token in request".to_owned()))?;

    let result = state.auth_service().refresh_session(&token).await?;

    let cookie_value = format!(
        "wardnet_session={}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
        result.token, result.max_age_seconds
    );

    Ok((StatusCode::NO_CONTENT, [(SET_COOKIE, cookie_value)]))
}

/// Extract the raw session token from a `wardnet_session` cookie or a
/// `Authorization: Bearer` header (cookie takes precedence).
fn extract_session_token(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get(axum::http::header::COOKIE)
        && let Some(token) = v.to_str().ok().and_then(|s| {
            s.split(';').find_map(|pair| {
                let (name, value) = pair.trim().split_once('=')?;
                if name.trim() == "wardnet_session" {
                    Some(value.trim().to_owned())
                } else {
                    None
                }
            })
        })
    {
        return Some(token);
    }
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}
