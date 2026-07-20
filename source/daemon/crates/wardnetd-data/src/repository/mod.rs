pub mod admin;
pub mod api_key;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod dns_events;
pub mod dns_filter;
pub mod dns_local;
pub mod inbound_wg;
pub mod maintenance;
pub mod network_zone;
pub mod notification;
pub mod private_dns;
pub mod push;
pub mod rule_request;
pub mod session;
pub mod sqlite;
pub mod stats;
pub mod system_config;
pub mod tunnel;
pub mod tunnel_speed_test;
pub mod update;
pub mod zone_exception;

pub use admin::AdminRepository;
pub use api_key::ApiKeyRepository;
pub use device::{DeviceRepository, DeviceRow};
pub use dhcp::{DhcpLeaseLogRow, DhcpLeaseRow, DhcpRepository, DhcpReservationRow};
pub use dns::{DnsRepository, QueryLogFilter, QueryLogRow};
pub use dns_events::{DnsCaptureStats, DnsEventRow, DnsEventsRepository};
pub use dns_filter::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate,
    DeviceSettingsRow, DeviceSettingsWithIp, DnsFilterRepository, ProfileFilterInputs,
};
pub use dns_local::{
    DnsLocalRepository, RecordRow, RecordUpdate, RuleRow, RuleUpdate, UpsertRecordRow, ZoneRow,
    ZoneUpdate,
};
pub use inbound_wg::{InboundWgPeerRepository, InboundWgPeerRow};
pub use maintenance::{MaintenanceRepository, WalCheckpointOutcome};
pub use network_zone::NetworkZoneRepository;
pub use notification::{NewNotification, NotificationRepository, StoredNotification};
pub use private_dns::{PrivateDnsGrantRepository, PrivateDnsGrantRow};
pub use push::{NewPushSubscription, PushRepository, StoredPushSubscription};
pub use rule_request::RuleRequestRepository;
pub use session::SessionRepository;
pub use sqlite::{
    SqliteAdminRepository, SqliteApiKeyRepository, SqliteDeviceRepository, SqliteDhcpRepository,
    SqliteDnsEventsRepository, SqliteDnsFilterRepository, SqliteDnsLocalRepository,
    SqliteDnsRepository, SqliteInboundWgPeerRepository, SqliteMaintenanceRepository,
    SqliteNetworkZoneRepository, SqliteNotificationRepository, SqlitePrivateDnsGrantRepository,
    SqlitePushRepository, SqliteRuleRequestRepository, SqliteSessionRepository,
    SqliteStatsRepository, SqliteSystemConfigRepository, SqliteTunnelRepository,
    SqliteTunnelSpeedTestRepository, SqliteUpdateRepository, SqliteZoneExceptionRepository,
};
pub use stats::{DailyStatRow, HourlyStatRow, IntradayStatRow, StatsRepository};
pub use system_config::{LastShutdownInfo, SystemConfigRepository};
pub use tunnel::{TunnelRepository, TunnelRow};
pub use tunnel_speed_test::{SpeedTestRow, TunnelSpeedTestRepository};
pub use update::{UpdateHistoryRow, UpdateRepository};
pub use zone_exception::ZoneExceptionRepository;

#[cfg(test)]
mod tests;
