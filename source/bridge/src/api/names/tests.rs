// Name-availability handler tests — validation logic lives in
// `crate::api::validation`; comprehensive tests are there. These tests
// verify that `name_available` correctly delegates to the shared validator.

#[test]
fn valid_name_is_valid() {
    assert!(crate::api::validation::is_valid_name("happy-einstein"));
    assert!(crate::api::validation::is_valid_name("abc"));
    assert!(crate::api::validation::is_valid_name("x1z"));
    assert!(crate::api::validation::is_valid_name(&"a".repeat(32)));
}

#[test]
fn invalid_names_rejected() {
    assert!(!crate::api::validation::is_valid_name("ab")); // too short
    assert!(!crate::api::validation::is_valid_name(&"a".repeat(33))); // too long
    assert!(!crate::api::validation::is_valid_name("-foo")); // leading hyphen
    assert!(!crate::api::validation::is_valid_name("foo-")); // trailing hyphen
    assert!(!crate::api::validation::is_valid_name("Foo")); // uppercase
    assert!(!crate::api::validation::is_valid_name("foo bar")); // space
    assert!(!crate::api::validation::is_valid_name("www")); // reserved
    assert!(!crate::api::validation::is_valid_name("admin")); // reserved
}
