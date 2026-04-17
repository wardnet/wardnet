//! # wardnetd-mock
//!
//! A standalone binary + library that runs the full Wardnet HTTP API on
//! loopback with **no** kernel or network side effects. Intended for
//! web-ui developers who need a realistic API surface without deploying
//! to a Raspberry Pi.
//!
//! ## Architecture
//!
//! The mock wires the same service layer used by the real daemon
//! (`wardnetd-services::init_services_with_factory`) against a set of
//! no-op [`backends`] that implement every trait (`TunnelInterface`,
//! `FirewallManager`, `PolicyRouter`, `PacketCapture`, `HostnameResolver`,
//! `KeyStore`, `DhcpServer`, `DnsServer`) but perform no real I/O.
//!
//! The database defaults to `:memory:`; an on-disk path can be supplied
//! via `--database` for sessions that should survive restart.
//!
//! Admin credentials are **not** seeded — the Setup Wizard runs on every
//! fresh launch so that flow can be exercised repeatedly during dev.
//!
//! ## Modules
//!
//! * [`backends`] — no-op implementations of network/OS backends
//! * [`seed`] — repository-level demo data population
//! * [`events`] — periodic fake event emitter for the event stream

pub mod backends;
pub mod events;
pub mod seed;

#[cfg(test)]
mod tests;
