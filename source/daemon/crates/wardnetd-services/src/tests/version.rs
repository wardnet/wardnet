use crate::version::{
    RELEASE_VERSION, VERSION, bump_patch, is_bare_semver, parse_git_describe, read_calver,
    read_calver_at,
};

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

#[test]
fn release_version_const_is_nonempty() {
    // Compile-time CalVer from the workspace-root `CALVER` file (via
    // each crate's build.rs). If `read_calver` ever wires up wrong,
    // the const would be empty here.
    assert!(!RELEASE_VERSION.is_empty());
}

#[test]
fn read_calver_resolves_workspace_file() {
    // Tests cwd is `source/daemon/crates/wardnetd-services/`, so the
    // hardcoded `../../../../CALVER` lands on the real workspace file.
    // It's enough to assert the result is non-empty and matches the
    // env-injected RELEASE_VERSION — content equality means the build
    // script and runtime are reading the same source of truth.
    let contents = read_calver();
    assert!(!contents.is_empty());
    assert_eq!(contents, RELEASE_VERSION);
}

#[test]
fn read_calver_at_trims_whitespace() {
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("wardnet-calver-test-{pid}"));
    std::fs::create_dir_all(&dir).expect("create tempdir");
    let path = dir.join("CALVER");
    // Mixed whitespace around a CalVer-shaped payload — the helper
    // must strip every kind of trailing/leading separator before
    // returning, so callers don't have to repeat the trim.
    std::fs::write(&path, "  2099.12.31\n\t\n").expect("write tempfile");
    let result = read_calver_at(&path);
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(result, "2099.12.31");
}

#[test]
#[should_panic(expected = "failed to read CALVER file")]
fn read_calver_at_panics_on_missing_file() {
    // `assert!(!trimmed.is_empty(), ...)` and the missing-file branch
    // both panic; this asserts the missing-file branch in particular.
    read_calver_at(std::path::Path::new(
        "/var/empty/no-such-file-wardnet-calver-test",
    ));
}

#[test]
#[should_panic(expected = "is empty")]
fn read_calver_at_panics_on_empty_file() {
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("wardnet-calver-empty-{pid}"));
    std::fs::create_dir_all(&dir).expect("create tempdir");
    let path = dir.join("CALVER");
    // File exists but parses to "" after trimming — the assert!
    // branch must fire so a build can't silently emit an empty
    // RELEASE_VERSION compile-time const.
    std::fs::write(&path, "   \n\t  \n").expect("write tempfile");
    let _ = read_calver_at(&path);
    // unreachable; tempdir cleanup happens in panic-tolerant teardown
}
