//! Per-egress-path health probing (issue #1338).
//!
//! Periodically completes a TCP connect *and* a TLS handshake plus a small
//! request over every egress path — each tunnel that is up, plus direct — and
//! publishes what came back.
//!
//! # Two invariants
//!
//! **Probing our own egress paths is not probing a device.** ADR 0025's rule —
//! nothing probes a device without a direct admin action — governs traffic
//! aimed at a household device. An egress path is Wardnet's own uplink;
//! measuring it sends packets to a public endpoint through our own interface
//! and touches no device on the network.
//!
//! **A failing probe is an anomaly and never an input to `HealthMonitor`.**
//! This is the rule ADR 0030 sets for a published app's reachability probe,
//! for the same reason: the health-gated soft watchdog restarts `wardnetd`, so
//! feeding it a signal that depends on a third party means a flaky VPN — or a
//! cloud outage — restarts the daemon. Nothing here implements `HealthCheck`,
//! and nothing here is registered with the monitor.

pub mod health;
pub mod prober;
pub mod runner;

pub use health::EgressPathHealth;
pub use prober::{EgressPathProber, ProbeTarget};
pub use runner::EgressPathProbeRunner;

#[cfg(test)]
mod tests;
