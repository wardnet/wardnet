//! Inbound (multi-peer) `WireGuard` server subsystem (issues #809, #810).
//!
//! Mirrors the outbound single-peer [`tunnel`](crate::tunnel) subsystem in
//! shape and conventions, but peer-list-shaped: one server interface
//! (`wg_wardin0`) with a singleton keypair, plus a growing/shrinking list of
//! admitted peers. Each peer is a remote-access grant on an already-managed
//! [`Device`](wardnet_common::device::Device) (#810): a live handshake flips
//! that device's `connection_mode` to `Remote` via the discovery service.

pub mod interface;
pub mod key_store;
pub mod keygen;
pub mod service;

pub use interface::{
    InboundWgInterface, InboundWgPeerConfig, InboundWgPeerStats, InboundWgServerConfig,
};
pub use key_store::{ServerKeyStore, ServerKeyStoreAdapter};
pub use service::{
    INBOUND_WG_INTERFACE, InboundWgMonitorPeer, InboundWgService, InboundWgServiceImpl,
};

#[cfg(test)]
mod tests;
