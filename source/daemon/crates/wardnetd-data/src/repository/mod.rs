pub mod admin;
pub mod api_key;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod dns_events;
pub mod dns_filter;
pub mod maintenance;
pub mod session;
pub mod sqlite;
pub mod stats;
pub mod system_config;
pub mod tunnel;
pub mod update;

pub use admin::AdminRepository;
pub use api_key::ApiKeyRepository;
pub use device::{DeviceRepository, DeviceRow};
pub use dhcp::{DhcpLeaseLogRow, DhcpLeaseRow, DhcpRepository, DhcpReservationRow};
pub use dns::{DnsRepository, QueryLogFilter, QueryLogRow};
pub use dns_events::{DnsCaptureStats, DnsEventsRepository};
pub use dns_filter::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate,
    DeviceSettingsRow, DeviceSettingsWithIp, DnsFilterRepository, ProfileFilterInputs,
};
pub use maintenance::MaintenanceRepository;
pub use session::SessionRepository;
pub use sqlite::{
    SqliteAdminRepository, SqliteApiKeyRepository, SqliteDeviceRepository, SqliteDhcpRepository,
    SqliteDnsEventsRepository, SqliteDnsFilterRepository, SqliteDnsRepository,
    SqliteMaintenanceRepository, SqliteSessionRepository, SqliteStatsRepository,
    SqliteSystemConfigRepository, SqliteTunnelRepository, SqliteUpdateRepository,
};
pub use stats::{DailyStatRow, HourlyStatRow, IntradayStatRow, StatsRepository};
pub use system_config::{LastShutdownInfo, SystemConfigRepository};
pub use tunnel::{TunnelRepository, TunnelRow};
pub use update::{UpdateHistoryRow, UpdateRepository};

#[cfg(test)]
mod tests;
