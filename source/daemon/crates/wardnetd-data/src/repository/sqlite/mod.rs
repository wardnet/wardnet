//! SQLite-backed repository implementations.
//!
//! Each module provides a concrete `Sqlite*Repository` struct that implements
//! the corresponding trait from the parent [`repository`](super) module.

pub mod admin;
pub mod api_key;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod dns_events;
pub mod dns_filter;
pub mod dns_local;
pub mod maintenance;
pub mod session;
pub mod stats;
pub mod system_config;
pub mod tunnel;
pub mod update;

pub use admin::SqliteAdminRepository;
pub use api_key::SqliteApiKeyRepository;
pub use device::SqliteDeviceRepository;
pub use dhcp::SqliteDhcpRepository;
pub use dns::SqliteDnsRepository;
pub use dns_events::SqliteDnsEventsRepository;
pub use dns_filter::SqliteDnsFilterRepository;
pub use dns_local::SqliteDnsLocalRepository;
pub use maintenance::SqliteMaintenanceRepository;
pub use session::SqliteSessionRepository;
pub use stats::SqliteStatsRepository;
pub use system_config::SqliteSystemConfigRepository;
pub use tunnel::SqliteTunnelRepository;
pub use update::SqliteUpdateRepository;

#[cfg(test)]
mod tests;
