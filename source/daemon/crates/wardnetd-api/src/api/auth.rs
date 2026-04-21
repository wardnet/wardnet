use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::response::IntoResponse;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wardnet_common::api::{ApiError, LoginRequest, LoginResponse};

use crate::state::AppState;
use wardnetd_services::error::AppError;

/// Register auth routes onto the given [`OpenApiRouter`]. Each module owns its
/// own route list so `api::mod::router` stays a simple composition point.
pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState> {
    router.routes(routes!(login))
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
        .login(&body.username, &body.password)
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
