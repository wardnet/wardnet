mod authoritative;
mod cache;
mod capture_runner;
mod dhcp_lan_runner;
mod filter_parser;
mod log_sink;
mod query_log_runner;
mod service;

use uuid::Uuid;
use wardnet_common::auth::AuthContext;

/// Admin auth context shared by the sibling test files.
pub(crate) fn admin() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::nil(),
    }
}
