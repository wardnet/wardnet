pub mod api;
pub mod auth;
pub mod backup;
pub mod config;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod dns_filter;
pub mod event;
pub mod jobs;
pub mod net;
pub mod network_zone;
pub mod routing;
pub mod routing_profile;
pub mod rule_request;
pub mod serde_util;
pub mod speed_test;
pub mod stats;
pub mod tunnel;
pub mod update;
pub mod vpn_provider;
pub mod wireguard_config;
pub mod zone_exception;

#[cfg(test)]
mod tests;
