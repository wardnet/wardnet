use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use wardnet_common::api::ApiError;

use crate::request_context;

/// Application-level error type that maps to HTTP responses.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("conflict: {0}")]
    Conflict(String),

    /// A precondition of the request is not satisfied — the operation is not
    /// wrong, the box is not currently in a state where it can be attempted.
    ///
    /// Used by the passkey ceremonies (ADR-0031 §8), which need a real domain
    /// and a secure context: on the plain-HTTP `:7411` surface, or when the
    /// request host does not match the pinned Relying Party ID, this is a `412`
    /// rather than a `400` or a `500`. The distinction matters because the fix
    /// is "reach Wardnet at its real hostname", not "send a different request".
    #[error("precondition failed: {0}")]
    PreconditionFailed(String),

    /// The caller is being rate-limited. Carries the number of seconds to wait,
    /// which is echoed in a `Retry-After` header so a client can back off
    /// correctly instead of hammering and extending its own lockout.
    ///
    /// Distinct from [`Self::Forbidden`] on purpose: a throttled login has not
    /// been judged wrong, only early, and a UI must be able to tell a user "try
    /// again shortly" rather than "your password is invalid".
    #[error("too many requests: {message}")]
    TooManyRequests {
        /// Human-readable explanation, surfaced as the response `detail`.
        message: String,
        /// Seconds the caller should wait before retrying.
        retry_after_seconds: u64,
    },

    /// An external service (release manifest host, provider API, etc.) failed
    /// in a way we want the caller to see verbatim. Mapped to 502 Bad Gateway
    /// and the string is surfaced in the response `detail` field.
    #[error("upstream unavailable: {0}")]
    UpstreamUnavailable(String),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message, detail) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "not found", Some(msg.clone())),
            Self::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, "unauthorized", Some(msg.clone()))
            }
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, "forbidden", Some(msg.clone())),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad request", Some(msg.clone())),
            Self::Conflict(msg) => (StatusCode::CONFLICT, "conflict", Some(msg.clone())),
            Self::PreconditionFailed(msg) => (
                StatusCode::PRECONDITION_FAILED,
                "precondition failed",
                Some(msg.clone()),
            ),
            Self::TooManyRequests { message, .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                "too many requests",
                Some(message.clone()),
            ),
            Self::UpstreamUnavailable(msg) => {
                // Log at warn-level with the cause in the message so the
                // recent-errors feed captures it; not a programmer bug so
                // not an error-level event.
                tracing::warn!("upstream unavailable: {msg}");
                (
                    StatusCode::BAD_GATEWAY,
                    "upstream unavailable",
                    Some(msg.clone()),
                )
            }
            Self::Internal(err) => {
                tracing::error!(error = %err, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error",
                    None,
                )
            }
            Self::Database(err) => {
                tracing::error!(error = %err, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error",
                    None,
                )
            }
        };

        let body = ApiError {
            error: error_message.to_owned(),
            detail,
            request_id: request_context::current_request_id(),
        };

        // `Retry-After` is part of the contract for a 429, not decoration: a
        // client that cannot read the delay will retry immediately and extend
        // its own backoff.
        if let Self::TooManyRequests {
            retry_after_seconds,
            ..
        } = &self
        {
            let mut response = (status, Json(body)).into_response();
            if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after_seconds.to_string()) {
                response
                    .headers_mut()
                    .insert(axum::http::header::RETRY_AFTER, value);
            }
            return response;
        }

        (status, Json(body)).into_response()
    }
}
