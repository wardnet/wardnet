pub mod access_request;
pub mod anomaly;
pub mod api_key;
pub mod device;
pub mod device_identification;
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
pub mod routing_profile;
pub mod session;
pub mod sqlite;
pub mod stats;
pub mod system_config;
pub mod tunnel;
pub mod tunnel_speed_test;
pub mod update;
pub mod user;
pub mod user_credential;
pub mod user_enrolment;
pub mod zone_exception;

pub use access_request::{AccessRequestRepository, DuplicateOpenAccessRequestError};
pub use anomaly::{ANOMALY_RETENTION_CAP, AnomalyRepository, NewAnomaly};
pub use api_key::ApiKeyRepository;
pub use device::{DeviceRepository, DeviceRow, PrunedDevice};
pub use device_identification::{DeviceIdentificationRepository, DeviceSignalRow};
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
pub use maintenance::{
    IncrementalVacuumOutcome, MaintenanceRepository, VacuumStop, WalCheckpointOutcome,
};
pub use network_zone::NetworkZoneRepository;
pub use notification::{NewNotification, NotificationRepository, StoredNotification};
pub use private_dns::{PrivateDnsGrantRepository, PrivateDnsGrantRow};
pub use push::{NewPushSubscription, PushRepository, StoredPushSubscription};
pub use routing_profile::{
    DeviceAssignment, RoutingProfileRepository, RoutingProfileRow, RoutingProfileUpdate,
    RoutingRuleRow, RoutingRuleUpdate,
};
pub use session::{SessionForRefresh, SessionPrincipal, SessionRepository, SessionSummary};
pub use sqlite::{
    SqliteAccessRequestRepository, SqliteAnomalyRepository, SqliteApiKeyRepository,
    SqliteDeviceIdentificationRepository, SqliteDeviceRepository, SqliteDhcpRepository,
    SqliteDnsEventsRepository, SqliteDnsFilterRepository, SqliteDnsLocalRepository,
    SqliteDnsRepository, SqliteInboundWgPeerRepository, SqliteMaintenanceRepository,
    SqliteNetworkZoneRepository, SqliteNotificationRepository, SqlitePrivateDnsGrantRepository,
    SqlitePushRepository, SqliteRoutingProfileRepository, SqliteSessionRepository,
    SqliteStatsRepository, SqliteSystemConfigRepository, SqliteTunnelRepository,
    SqliteTunnelSpeedTestRepository, SqliteUpdateRepository, SqliteUserCredentialRepository,
    SqliteUserEnrolmentRepository, SqliteUserRepository, SqliteZoneExceptionRepository,
};
pub use stats::{DailyStatRow, HourlyStatRow, IntradayStatRow, StatsRepository};
pub use system_config::{LastShutdownInfo, SystemConfigRepository};
pub use tunnel::{TunnelRepository, TunnelRow};
pub use tunnel_speed_test::{SpeedTestRow, TunnelSpeedTestRepository};
pub use update::{UpdateHistoryRow, UpdateRepository};
pub use user::{DuplicateUserEmailError, UserRepository, UserRole, UserRow};
pub use user_credential::{
    CredentialAlreadyLinkedError, CredentialKind, CredentialLogin, CredentialRow,
    CredentialSummary, UserAlreadyHasPasswordError, UserCredentialRepository,
};
pub use user_enrolment::{EnrolmentTokenRow, UserEnrolmentRepository};
pub use zone_exception::ZoneExceptionRepository;

#[cfg(test)]
mod tests;
