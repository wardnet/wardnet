use super::{
    MAX_ACME_VALUE_LEN, RESERVED_NAMES, is_valid_name, validate_acme_values, validate_name,
};

// ── is_valid_name ─────────────────────────────────────────────────────────────

#[test]
fn valid_names_accepted() {
    assert!(is_valid_name("happy-einstein"));
    assert!(is_valid_name("abc"));
    assert!(is_valid_name("x1z"));
    assert!(is_valid_name(&"a".repeat(32)));
}

#[test]
fn too_short_or_long() {
    assert!(!is_valid_name("ab"));
    assert!(!is_valid_name(&"a".repeat(33)));
}

#[test]
fn hyphen_edges() {
    assert!(!is_valid_name("-foo"));
    assert!(!is_valid_name("foo-"));
}

#[test]
fn invalid_characters() {
    assert!(!is_valid_name("Foo"));
    assert!(!is_valid_name("foo bar"));
    assert!(!is_valid_name("foo_bar"));
}

#[test]
fn reserved_names_unavailable() {
    for name in RESERVED_NAMES {
        assert!(!is_valid_name(name), "'{name}' should be reserved");
    }
}

// ── validate_name ─────────────────────────────────────────────────────────────

#[test]
fn validate_name_ok() {
    assert!(validate_name("happy-einstein").is_ok());
    assert!(validate_name("abc").is_ok());
    assert!(validate_name(&"a".repeat(32)).is_ok());
}

#[test]
fn validate_name_length_errors() {
    assert!(validate_name("ab").is_err());
    assert!(validate_name(&"a".repeat(33)).is_err());
}

#[test]
fn validate_name_hyphen_errors() {
    assert!(validate_name("-foo").is_err());
    assert!(validate_name("foo-").is_err());
}

#[test]
fn validate_name_reserved_errors() {
    assert!(validate_name("www").is_err());
    assert!(validate_name("admin").is_err());
    assert!(validate_name("us").is_err());
}

// ── validate_acme_values ──────────────────────────────────────────────────────

#[test]
fn validate_acme_values_accepts_one_or_two() {
    assert!(validate_acme_values(&["apex".to_string()]).is_ok());
    assert!(validate_acme_values(&["apex".to_string(), "wildcard".to_string()]).is_ok());
}

#[test]
fn validate_acme_values_rejects_empty() {
    // Empty via the set endpoint is meaningless — callers clear via DELETE.
    assert!(validate_acme_values(&[]).is_err());
}

#[test]
fn validate_acme_values_rejects_too_many() {
    // The cross-tenant DoS guard: an oversized list is rejected before any CF write.
    let many: Vec<String> = (0..5).map(|i| i.to_string()).collect();
    assert!(validate_acme_values(&many).is_err());
}

#[test]
fn validate_acme_values_rejects_overlong_value() {
    let long = "a".repeat(MAX_ACME_VALUE_LEN + 1);
    assert!(validate_acme_values(&[long]).is_err());
}
