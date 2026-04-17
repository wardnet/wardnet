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

        (status, Json(body)).into_response()
    }
}
