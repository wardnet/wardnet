//! SQLite-backed repository implementations.
//!
//! Each module provides a concrete `Sqlite*Repository` struct that implements
//! the corresponding trait from the parent [`repository`](super) module.

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
pub mod rule_request;
pub mod session;
pub mod stats;
pub mod system_config;
pub mod tunnel;
pub mod tunnel_speed_test;
pub mod update;
pub mod user;
pub mod user_credential;
pub mod user_enrolment;
pub mod zone_exception;

pub use api_key::SqliteApiKeyRepository;
pub use device::SqliteDeviceRepository;
pub use device_identification::SqliteDeviceIdentificationRepository;
pub use dhcp::SqliteDhcpRepository;
pub use dns::SqliteDnsRepository;
pub use dns_events::SqliteDnsEventsRepository;
pub use dns_filter::SqliteDnsFilterRepository;
pub use dns_local::SqliteDnsLocalRepository;
pub use inbound_wg::SqliteInboundWgPeerRepository;
pub use maintenance::SqliteMaintenanceRepository;
pub use network_zone::SqliteNetworkZoneRepository;
pub use notification::SqliteNotificationRepository;
pub use private_dns::SqlitePrivateDnsGrantRepository;
pub use push::SqlitePushRepository;
pub use routing_profile::SqliteRoutingProfileRepository;
pub use rule_request::SqliteRuleRequestRepository;
pub use session::SqliteSessionRepository;
pub use stats::SqliteStatsRepository;
pub use system_config::SqliteSystemConfigRepository;
pub use tunnel::SqliteTunnelRepository;
pub use tunnel_speed_test::SqliteTunnelSpeedTestRepository;
pub use update::SqliteUpdateRepository;
pub use user::SqliteUserRepository;
pub use user_credential::SqliteUserCredentialRepository;
pub use user_enrolment::SqliteUserEnrolmentRepository;
pub use zone_exception::SqliteZoneExceptionRepository;

#[cfg(test)]
mod tests;
