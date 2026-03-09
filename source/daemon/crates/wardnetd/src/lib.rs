pub mod api;
pub mod bootstrap;
pub mod config;
pub mod db;
pub mod device_detector;
pub mod error;
pub mod event;
pub mod hostname_resolver;
pub mod hostname_resolver_noop;
pub mod keys;
pub mod oui;
pub mod packet_capture;
pub mod packet_capture_noop;
pub mod packet_capture_pnet;
pub mod repository;
pub mod service;
pub mod state;
pub mod tunnel_idle;
pub mod tunnel_monitor;
pub mod version;
pub mod web;
pub mod wireguard;
pub mod wireguard_noop;
pub mod wireguard_real;

#[cfg(test)]
mod tests;
