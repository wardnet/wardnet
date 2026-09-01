mod authoritative;
mod cache;
mod capture_runner;
mod dhcp_lan_runner;
mod filter_parser;
mod log_sink;
mod query_log_runner;
mod service;
mod upstream_health;

use wardnet_common::auth::AuthContext;

/// Admin auth context shared by the sibling test files.
pub(crate) fn admin() -> AuthContext {
    AuthContext::system()
}

/// Parse a whole-second RFC 3339 literal into the `QueryLogRow` timestamp type.
pub(crate) fn ts(literal: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(literal)
        .expect("test timestamp literal is valid RFC 3339")
        .with_timezone(&chrono::Utc)
}
