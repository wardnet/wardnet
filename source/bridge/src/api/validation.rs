//! Shared name and public-key validation used by the registration and
//! name-availability endpoints.
//!
//! Keeping validation in one place ensures that the availability check
//! (`GET /v1/names/{name}/available`) and the registration handler
//! (`POST /v1/register`) apply identical rules.

use crate::error::ApiError;

/// Subdomain names that may not be claimed by any installation.
///
/// Includes DNS infrastructure names, well-known service labels, and region
/// codes used as top-level subdomain components.
pub(crate) const RESERVED_NAMES: &[&str] = &[
    "www", "mail", "api", "ddns", "my", "admin", "bridge", "static",
    "wildcard", "wardnet", "support", "help", "ns", "ns1", "ns2",
    "ftp", "smtp", "imap", "pop3", "us", "eu",
];

/// Returns `true` when `name` satisfies all naming constraints.
///
/// Used by the availability endpoint which needs a `bool` result (not an
/// error) so it can return `{ "available": false }` for invalid names.
pub(crate) fn is_valid_name(name: &str) -> bool {
    let len = name.len();
    if !(3..=32).contains(&len) {
        return false;
    }
    if name.starts_with('-') || name.ends_with('-') {
        return false;
    }
    if !name.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-')) {
        return false;
    }
    !RESERVED_NAMES.contains(&name)
}

/// Validate a base64-encoded Ed25519 public key.
///
/// Accepts only exactly 32 bytes of valid base64-encoded data (the raw key
/// material of an Ed25519 verifying key). The actual Ed25519 key is not parsed
/// here — that happens in the auth middleware on authenticated requests.
pub(crate) fn validate_public_key(public_key: &str) -> Result<(), ApiError> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(public_key)
        .map_err(|_| ApiError::BadRequest("public_key is not valid base64".to_string()))?;
    if bytes.len() != 32 {
        return Err(ApiError::BadRequest(
            "public_key must be a base64-encoded Ed25519 key (32 bytes)".to_string(),
        ));
    }
    Ok(())
}

/// Validate `name` for use in registration, returning an [`ApiError`] on
/// failure.
///
/// Stricter than [`is_valid_name`] — same logic but with structured error
/// messages so the client knows exactly what was wrong.
pub(crate) fn validate_name(name: &str) -> Result<(), ApiError> {
    let len = name.len();
    if !(3..=32).contains(&len) {
        return Err(ApiError::BadRequest(
            "name must be between 3 and 32 characters".to_string(),
        ));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err(ApiError::BadRequest(
            "name must not start or end with a hyphen".to_string(),
        ));
    }
    if !name.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-')) {
        return Err(ApiError::BadRequest(
            "name may only contain lowercase letters, digits, and hyphens".to_string(),
        ));
    }
    if RESERVED_NAMES.contains(&name) {
        return Err(ApiError::BadRequest(format!("'{name}' is a reserved name")));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
