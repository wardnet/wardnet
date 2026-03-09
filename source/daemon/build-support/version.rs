// Shared version-parsing helpers used by both `build.rs` and unit tests.
//
// This file is `include!`d from `build.rs` and from `src/version.rs` so the
// logic exists in exactly one place. Functions use `pub(crate)` visibility so
// they are accessible from test modules when included in the library.

/// Parse the output of `git describe --tags --always --dirty` into a `SemVer`
/// version string.
///
/// Supported formats:
/// - `v0.1.0`                    -> `0.1.0`
/// - `v0.1.0-dirty`             -> `0.1.0-dirty`
/// - `v0.1.0-5-gabc1234`        -> `0.1.1-dev.5+gabc1234`
/// - `v0.1.0-5-gabc1234-dirty`  -> `0.1.1-dev.5.dirty+gabc1234`
/// - `abc1234`                   -> `CARGO_PKG_VERSION-dev+gabc1234`
/// - `abc1234-dirty`             -> `CARGO_PKG_VERSION-dev.dirty+gabc1234`
pub(crate) fn parse_git_describe(describe: &str) -> String {
    // Case: no tags — just a commit hash, possibly with -dirty.
    if !describe.starts_with('v') {
        return no_tag_version(describe);
    }

    // Strip the 'v' prefix.
    let rest = &describe[1..];

    // Try to parse as exact tag match: "MAJOR.MINOR.PATCH" or
    // "MAJOR.MINOR.PATCH-dirty".
    if let Some(version) = rest.strip_suffix("-dirty") {
        if is_bare_semver(version) {
            return format!("{version}-dirty");
        }
    } else if is_bare_semver(rest) {
        return rest.to_owned();
    }

    // Format: "MAJOR.MINOR.PATCH-N-gHASH" or "MAJOR.MINOR.PATCH-N-gHASH-dirty"
    let dirty = rest.ends_with("-dirty");
    let without_dirty = if dirty {
        rest.strip_suffix("-dirty").unwrap()
    } else {
        rest
    };

    // Split from the right: find the hash segment (gHASH) and commit count.
    if let Some(hash_pos) = without_dirty.rfind("-g") {
        let hash = &without_dirty[hash_pos + 1..]; // "gabc1234"
        let before_hash = &without_dirty[..hash_pos]; // "0.1.0-5"

        if let Some(count_pos) = before_hash.rfind('-') {
            let base_version = &before_hash[..count_pos]; // "0.1.0"
            let count = &before_hash[count_pos + 1..]; // "5"

            if is_bare_semver(base_version) {
                let bumped = bump_patch(base_version);
                let dirty_suffix = if dirty { ".dirty" } else { "" };
                return format!("{bumped}-dev.{count}{dirty_suffix}+{hash}");
            }
        }
    }

    // Fallback: could not parse, use Cargo.toml version.
    cargo_pkg_version()
}

/// Build a version string when there are no tags at all.
///
/// Input is either `abc1234` or `abc1234-dirty`.
fn no_tag_version(describe: &str) -> String {
    let base = cargo_pkg_version();
    if let Some(hash) = describe.strip_suffix("-dirty") {
        format!("{base}-dev.dirty+g{hash}")
    } else {
        format!("{base}-dev+g{describe}")
    }
}

/// Read the version from `Cargo.toml` via the standard env var that cargo sets
/// for build scripts.
fn cargo_pkg_version() -> String {
    std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".to_owned())
}

/// Check whether a string is a bare `MAJOR.MINOR.PATCH` version (digits and
/// dots only, three parts).
pub(crate) fn is_bare_semver(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Increment the patch component of a `MAJOR.MINOR.PATCH` string.
pub(crate) fn bump_patch(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() == 3
        && let Ok(patch) = parts[2].parse::<u64>()
    {
        return format!("{}.{}.{}", parts[0], parts[1], patch + 1);
    }
    version.to_owned()
}
