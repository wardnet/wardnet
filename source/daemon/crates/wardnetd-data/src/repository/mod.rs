pub mod admin;
pub mod api_key;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod dns_filter;
pub mod session;
pub mod sqlite;
pub mod stats;
pub mod system_config;
pub mod tunnel;
pub mod tunnel_metrics;
pub mod update;

pub use admin::AdminRepository;
pub use api_key::ApiKeyRepository;
pub use device::{DeviceRepository, DeviceRow};
pub use dhcp::{DhcpLeaseLogRow, DhcpLeaseRow, DhcpRepository, DhcpReservationRow};
pub use dns::{
    BucketSize, DnsRepository, QueryLogFilter, QueryLogRow, QueryStatsRow, SeriesBucketRow,
    TopClientRow, TopDomainRow,
};
pub use dns_filter::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate,
    DeviceSettingsRow, DeviceSettingsWithIp, DnsFilterRepository, ProfileFilterInputs,
};
pub use session::SessionRepository;
pub use sqlite::{
    SqliteAdminRepository, SqliteApiKeyRepository, SqliteDeviceRepository, SqliteDhcpRepository,
    SqliteDnsFilterRepository, SqliteDnsRepository, SqliteSessionRepository, SqliteStatsRepository,
    SqliteSystemConfigRepository, SqliteTunnelMetricsRepository, SqliteTunnelRepository,
    SqliteUpdateRepository,
};
pub use stats::{DailyStatRow, IntradayStatRow, StatsRepository};
pub use system_config::{LastShutdownInfo, SystemConfigRepository};
pub use tunnel::{TunnelRepository, TunnelRow};
pub use tunnel_metrics::{DailyMetricRow, IntradayMetricRow, TunnelMetricsRepository};
pub use update::{UpdateHistoryRow, UpdateRepository};

#[cfg(test)]
mod tests;
