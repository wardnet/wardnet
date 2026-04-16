pub mod admin;
pub mod api_key;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod session;
pub mod sqlite;
pub mod system_config;
pub mod tunnel;

pub use admin::AdminRepository;
pub use api_key::ApiKeyRepository;
pub use device::{DeviceRepository, DeviceRow};
pub use dhcp::{DhcpLeaseLogRow, DhcpLeaseRow, DhcpRepository, DhcpReservationRow};
pub use dns::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate, DnsRepository,
    QueryLogFilter, QueryLogRow,
};
pub use session::SessionRepository;
pub use sqlite::{
    SqliteAdminRepository, SqliteApiKeyRepository, SqliteDeviceRepository, SqliteDhcpRepository,
    SqliteDnsRepository, SqliteSessionRepository, SqliteSystemConfigRepository,
    SqliteTunnelRepository,
};
pub use system_config::SystemConfigRepository;
pub use tunnel::{TunnelRepository, TunnelRow};

#[cfg(test)]
mod tests;
