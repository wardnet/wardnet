use crate::version::{VERSION, bump_patch, is_bare_semver, parse_git_describe};

#[test]
fn exact_tag() {
    assert_eq!(parse_git_describe("v0.1.0"), "0.1.0");
    assert_eq!(parse_git_describe("v1.2.3"), "1.2.3");
}

#[test]
fn exact_tag_dirty() {
    assert_eq!(parse_git_describe("v0.1.0-dirty"), "0.1.0-dirty");
}

#[test]
fn commits_after_tag() {
    assert_eq!(
        parse_git_describe("v0.1.0-5-gabc1234"),
        "0.1.1-dev.5+gabc1234"
    );
}

#[test]
fn commits_after_tag_dirty() {
    assert_eq!(
        parse_git_describe("v0.1.0-5-gabc1234-dirty"),
        "0.1.1-dev.5.dirty+gabc1234"
    );
}

#[test]
fn no_tags_hash_only() {
    let result = parse_git_describe("abc1234");
    assert!(result.ends_with("-dev+gabc1234"), "got: {result}");
}

#[test]
fn no_tags_hash_dirty() {
    let result = parse_git_describe("abc1234-dirty");
    assert!(result.ends_with("-dev.dirty+gabc1234"), "got: {result}");
}

#[test]
fn bump_patch_increments() {
    assert_eq!(bump_patch("0.1.0"), "0.1.1");
    assert_eq!(bump_patch("1.2.99"), "1.2.100");
}

#[test]
fn is_bare_semver_valid() {
    assert!(is_bare_semver("0.1.0"));
    assert!(is_bare_semver("10.20.30"));
}

#[test]
fn is_bare_semver_invalid() {
    assert!(!is_bare_semver("0.1"));
    assert!(!is_bare_semver("0.1.0-beta"));
    assert!(!is_bare_semver(""));
}

#[test]
fn version_const_is_nonempty() {
    assert!(!VERSION.is_empty());
}
