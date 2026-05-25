//! SQLite-backed repository implementations.
//!
//! Each module provides a concrete `Sqlite*Repository` struct that implements
//! the corresponding trait from the parent [`repository`](super) module.

pub mod admin;
pub mod api_key;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod dns_filter;
pub mod session;
pub mod stats;
pub mod system_config;
pub mod tunnel;
pub mod tunnel_metrics;
pub mod update;

pub use admin::SqliteAdminRepository;
pub use api_key::SqliteApiKeyRepository;
pub use device::SqliteDeviceRepository;
pub use dhcp::SqliteDhcpRepository;
pub use dns::SqliteDnsRepository;
pub use dns_filter::SqliteDnsFilterRepository;
pub use session::SqliteSessionRepository;
pub use stats::SqliteStatsRepository;
pub use system_config::SqliteSystemConfigRepository;
pub use tunnel::SqliteTunnelRepository;
pub use tunnel_metrics::SqliteTunnelMetricsRepository;
pub use update::SqliteUpdateRepository;
