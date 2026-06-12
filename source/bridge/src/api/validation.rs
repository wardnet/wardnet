//! Shared name and public-key validation used by the registration and
//! name-availability endpoints.
//!
//! Keeping validation in one place ensures that the availability check
//! (`GET /v1/names/{name}/available`) and the registration handler
//! (`POST /v1/register`) apply identical rules.

use crate::error::ApiError;

/// Maximum number of ACME challenge values a single set request may carry.
///
/// A **per-user wildcard certificate** authorizes exactly two SANs (the apex +
/// the wildcard) through the one `_acme-challenge` name, so two values is the
/// real shape; the small margin tolerates a future extra SAN without uncapping.
/// The cap is the trust boundary: the daemon limits itself, but a malicious
/// install calls the bridge directly, and each value fans out to a Cloudflare
/// TXT create against the region's **shared** zone — an uncapped list is a
/// cross-tenant `DoS` on that shared CF rate budget.
const MAX_ACME_VALUES: usize = 4;

/// Maximum length of a single ACME challenge value. A DNS-01 key authorization
/// digest is 43 base64url chars; 255 is the DNS TXT single-string limit and a
/// generous defence-in-depth bound on the request body.
const MAX_ACME_VALUE_LEN: usize = 255;

/// Subdomain names that may not be claimed by any installation.
///
/// Includes DNS infrastructure names, well-known service labels, and region
/// codes used as top-level subdomain components.
pub(crate) const RESERVED_NAMES: &[&str] = &[
    "www", "mail", "api", "ddns", "my", "admin", "bridge", "static", "wildcard", "wardnet",
    "support", "help", "ns", "ns1", "ns2", "ftp", "smtp", "imap", "pop3", "us", "eu",
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
    if !name
        .chars()
        .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
    {
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
    if !name
        .chars()
        .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
    {
        return Err(ApiError::BadRequest(
            "name may only contain lowercase letters, digits, and hyphens".to_string(),
        ));
    }
    if RESERVED_NAMES.contains(&name) {
        return Err(ApiError::BadRequest(format!("'{name}' is a reserved name")));
    }
    Ok(())
}

/// Validate the ACME challenge value list from `PUT /v1/installs/{id}/acme-challenge`.
///
/// Bounds an *authenticated* caller's request before any Cloudflare write: the
/// list must be non-empty (an empty set via the *set* endpoint is meaningless —
/// callers clear via `DELETE`) and no longer than [`MAX_ACME_VALUES`], and each
/// value within [`MAX_ACME_VALUE_LEN`]. See [`MAX_ACME_VALUES`] for why the cap
/// is a cross-tenant safety boundary, not just input hygiene.
pub(crate) fn validate_acme_values(values: &[String]) -> Result<(), ApiError> {
    if values.is_empty() {
        return Err(ApiError::BadRequest(
            "values must contain at least one challenge value (clear via DELETE)".to_string(),
        ));
    }
    if values.len() > MAX_ACME_VALUES {
        return Err(ApiError::BadRequest(format!(
            "at most {MAX_ACME_VALUES} challenge values may be set at once"
        )));
    }
    if let Some(v) = values.iter().find(|v| v.len() > MAX_ACME_VALUE_LEN) {
        return Err(ApiError::BadRequest(format!(
            "challenge value exceeds {MAX_ACME_VALUE_LEN} characters ({} given)",
            v.len()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
