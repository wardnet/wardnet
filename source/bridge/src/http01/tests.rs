use super::is_valid_acme_token;

#[test]
fn accepts_realistic_token() {
    // A typical Let's Encrypt HTTP-01 token (base64url, 43 chars).
    assert!(is_valid_acme_token(
        "evaGxfADs6pSRb2Lav9IZf6DuOmjmQAfW ".trim()
    ));
    assert!(is_valid_acme_token("abcXYZ012-_"));
}

#[test]
fn rejects_empty() {
    assert!(!is_valid_acme_token(""));
}

#[test]
fn rejects_overlong() {
    assert!(!is_valid_acme_token(&"a".repeat(129)));
}

#[test]
fn rejects_path_traversal_and_specials() {
    assert!(!is_valid_acme_token("../../etc/passwd"));
    assert!(!is_valid_acme_token("token with space"));
    assert!(!is_valid_acme_token("token/slash"));
    assert!(!is_valid_acme_token("token.dot"));
    assert!(!is_valid_acme_token("token%2e"));
}
