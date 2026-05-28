use super::{RESERVED_NAMES, is_valid_name, validate_name};

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
