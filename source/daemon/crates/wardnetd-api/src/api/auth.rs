use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::SET_COOKIE;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{ApiError, LoginRequest, LoginResponse};

use crate::api::middleware::{AdminAuth, SecureTransport};
use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Build the `wardnet_session` `Set-Cookie` value.
///
/// `Secure` is appended only when the request arrived over TLS — signalled by
/// the [`SecureTransport`] marker that `guarded_https_app` layers onto the
/// `:443` app. The plain-HTTP `:7411` surface (and the mock/dev server) omit it,
/// because browsers refuse to store a `Secure` cookie delivered over `http://`.
fn session_cookie(token: &str, max_age_seconds: u64, secure: bool) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!(
        "wardnet_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age_seconds}{secure_attr}"
    )
}

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
    secure_transport: Option<Extension<SecureTransport>>,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let result = state
        .auth_service()
        .login(&body.username, &body.password, body.remember_me)
        .await?;

    let cookie_value = session_cookie(
        &result.token,
        result.max_age_seconds,
        secure_transport.is_some(),
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
    secure_transport: Option<Extension<SecureTransport>>,
    auth: AdminAuth,
) -> Result<impl IntoResponse, AppError> {
    let token = auth
        .session_token
        .ok_or_else(|| AppError::Unauthorized("no session token in request".to_owned()))?;

    let result = state.auth_service().refresh_session(&token).await?;

    let cookie_value = session_cookie(
        &result.token,
        result.max_age_seconds,
        secure_transport.is_some(),
    );

    Ok((StatusCode::NO_CONTENT, [(SET_COOKIE, cookie_value)]))
}
