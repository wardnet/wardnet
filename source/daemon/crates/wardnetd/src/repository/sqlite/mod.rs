//! SQLite-backed repository implementations.
//!
//! Each module provides a concrete `Sqlite*Repository` struct that implements
//! the corresponding trait from the parent [`repository`](super) module.

pub mod admin;
pub mod api_key;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod session;
pub mod system_config;
pub mod tunnel;

pub use admin::SqliteAdminRepository;
pub use api_key::SqliteApiKeyRepository;
pub use device::SqliteDeviceRepository;
pub use dhcp::SqliteDhcpRepository;
pub use dns::SqliteDnsRepository;
pub use session::SqliteSessionRepository;
pub use system_config::SqliteSystemConfigRepository;
pub use tunnel::SqliteTunnelRepository;
