use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::response::IntoResponse;
use wardnet_types::api::{LoginRequest, LoginResponse};

use crate::error::AppError;
use crate::state::AppState;

/// POST /api/auth/login
///
/// Thin handler — delegates credential verification and session creation
/// to [`AuthService`](crate::service::AuthService), then sets the session cookie.
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
        }),
    ))
}
