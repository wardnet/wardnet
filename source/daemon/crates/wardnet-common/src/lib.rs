pub mod api;
pub mod auth;
pub mod backup;
pub mod config;
pub mod device;
pub mod dhcp;
pub mod dns;
pub mod event;
pub mod jobs;
pub mod routing;
pub mod tunnel;
pub mod update;
pub mod vpn_provider;
pub mod wireguard_config;

#[cfg(test)]
mod tests;
