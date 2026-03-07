use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::HeaderMap;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::error::AppError;
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
