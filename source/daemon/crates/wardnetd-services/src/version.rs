/// Git-derived version string set at compile time by `build.rs`.
///
/// Format: `MAJOR.MINOR.PATCH` for tagged builds, with `-dev.N+gHASH`
/// suffix on commits past a tag. Use this for diagnostics where the
/// exact commit matters (CLI `--version`, log spans, OTLP attributes).
pub const VERSION: &str = env!("WARDNET_VERSION");

/// Public-facing CalVer (`YYYY.MM.DD`) read at compile time from the
/// workspace-root `CALVER` file. Stable across dev rebuilds — use this
/// for anything user-visible (web UI, OpenAPI `info.version`,
/// /api/info `release_version`) or for the auto-update version
/// comparison against the published manifest.
pub const RELEASE_VERSION: &str = env!("WARDNET_RELEASE_VERSION");

// Include the shared parsing helpers so they can be exercised by unit tests
// without duplicating any logic.
#[cfg(test)]
include!("../../../build-support/version.rs");
