/// Git-derived version string set at compile time by `build.rs`.
pub const VERSION: &str = env!("WARDNET_VERSION");

// Include the shared parsing helpers so they can be exercised by unit tests
// without duplicating any logic.
#[cfg(test)]
include!("../../../build-support/version.rs");
