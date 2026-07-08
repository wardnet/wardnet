//! Inbound (multi-peer) `WireGuard` server subsystem (issue #809).
//!
//! Mirrors the outbound single-peer [`tunnel`](crate::tunnel) subsystem in
//! shape and conventions, but peer-list-shaped: one server interface
//! (`wg_wardin0`) with a singleton keypair, plus a growing/shrinking list of
//! admitted peers. Explicitly NOT wired into the device / routing / zone model
//! — peers get a fixed static route only (a separate future issue owns that).

pub mod interface;
pub mod key_store;
pub mod keygen;
pub mod service;

pub use interface::{
    InboundWgInterface, InboundWgPeerConfig, InboundWgPeerStats, InboundWgServerConfig,
};
pub use key_store::{ServerKeyStore, ServerKeyStoreAdapter};
pub use service::{INBOUND_WG_INTERFACE, InboundWgService, InboundWgServiceImpl};

#[cfg(test)]
mod tests;
