// Validation helper tests for POST /v1/register — validation logic now lives
// in `crate::api::validation`; these tests verify the shared functions.
// Full-stack API tests live in tests/api.rs.

use crate::api::validation::{validate_name, validate_public_key};

/// `validate_name` should accept a well-formed name.
#[test]
fn valid_name_accepted() {
    assert!(validate_name("happy-einstein").is_ok());
    assert!(validate_name("abc").is_ok());
    assert!(validate_name("x1z").is_ok());
}

/// Reserved names must be rejected.
#[test]
fn reserved_names_rejected() {
    assert!(validate_name("www").is_err());
    assert!(validate_name("admin").is_err());
    assert!(validate_name("us").is_err());
}

/// Names outside the length range must be rejected.
#[test]
fn name_length_bounds() {
    // too short
    assert!(validate_name("ab").is_err());
    // too long
    let long = "a".repeat(33);
    assert!(validate_name(&long).is_err());
    // boundary values
    assert!(validate_name("abc").is_ok());
    assert!(validate_name(&"a".repeat(32)).is_ok());
}

/// Leading/trailing hyphens are invalid.
#[test]
fn hyphen_edges_rejected() {
    assert!(validate_name("-foo").is_err());
    assert!(validate_name("foo-").is_err());
}

/// Public-key validation: wrong length and non-base64 must be rejected.
#[test]
fn public_key_validation() {
    use base64::Engine as _;
    // 32 bytes base64-encoded → valid
    let valid = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
    assert!(validate_public_key(&valid).is_ok());
    // 31 bytes → wrong length
    let short = base64::engine::general_purpose::STANDARD.encode([0u8; 31]);
    assert!(validate_public_key(&short).is_err());
    // garbage → invalid base64
    assert!(validate_public_key("!!!").is_err());
}
