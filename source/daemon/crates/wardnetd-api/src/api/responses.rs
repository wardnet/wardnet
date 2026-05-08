//! Shared response types for `#[utoipa::path(...)]` annotations.
//!
//! These are purely documentation helpers — they never materialize at runtime
//! because handlers return `Result<Json<T>, AppError>`, and `AppError`'s
//! `IntoResponse` is what actually builds the HTTP error bodies. The types
//! here exist so every authenticated endpoint can reference one name instead
//! of repeating the same 401/403/500 tuples.

use utoipa::IntoResponses;
use wardnet_common::api::ApiError;

/// 401/403/500 — the error set every authenticated endpoint can return.
#[derive(IntoResponses)]
#[allow(dead_code)]
pub enum AuthErrors {
    /// Missing or invalid session cookie / API key.
    #[response(
        status = 401,
        description = "Unauthenticated — session cookie or API key missing/invalid"
    )]
    Unauthorized(#[to_schema] ApiError),
    /// Caller authenticated but lacks admin role.
    #[response(status = 403, description = "Forbidden — caller is not an admin")]
    Forbidden(#[to_schema] ApiError),
    /// Unhandled server-side failure.
    #[response(status = 500, description = "Internal server error")]
    Internal(#[to_schema] ApiError),
}

/// 404 — use on endpoints that accept a path identifier.
#[derive(IntoResponses)]
#[allow(dead_code)]
pub enum NotFound {
    /// Resource matching the given identifier was not found.
    #[response(status = 404, description = "Resource not found")]
    NotFound(#[to_schema] ApiError),
}

/// 400 — use on endpoints that parse a JSON request body.
#[derive(IntoResponses)]
#[allow(dead_code)]
pub enum BadRequest {
    /// Malformed JSON or failed validation.
    #[response(status = 400, description = "Malformed or invalid request body")]
    BadRequest(#[to_schema] ApiError),
}

/// 409 — use on endpoints that may collide with concurrent state.
#[derive(IntoResponses)]
#[allow(dead_code)]
pub enum Conflict {
    /// Another operation conflicts with this request.
    #[response(status = 409, description = "Conflicting concurrent operation")]
    Conflict(#[to_schema] ApiError),
}

/// 502 — use on endpoints that depend on an external service.
#[derive(IntoResponses)]
#[allow(dead_code)]
pub enum UpstreamUnavailable {
    /// Upstream service was unreachable or returned an unexpected response.
    #[response(
        status = 502,
        description = "Upstream service unavailable or unreachable"
    )]
    UpstreamUnavailable(#[to_schema] ApiError),
}
