//! Shared per-upstream reachability, published by the DNS server's latency
//! prober and read by everything that needs to know which upstreams are
//! currently answering.
//!
//! This exists as its own handle rather than as a method on
//! [`DnsServer`](crate::dns::DnsServer) because of construction order: the
//! anomaly detector registry is built during service wiring, while the DNS
//! server is constructed later by the daemon binary. A handle both sides are
//! given a clone of lets the detector read the prober's findings without the
//! registry holding a reference to a server that does not exist yet — and
//! without an `Arc` cycle between them.
//!
//! Honest limit on what `reachable` means: the prober measures its *own*
//! connection to each upstream, deliberately kept separate from the ones
//! serving queries so live traffic cannot skew the timing (for `DoT`/`DoH`,
//! where a connection is persistent and multiplexed, a shared one would
//! queue the probe behind real queries and measure latency-plus-queueing).
//! The cost of that independence is that a probe's success is not proof the
//! serving path is healthy: an encrypted upstream whose query connection has
//! wedged can still answer probes. Reachability is therefore a routing
//! *hint*. What actually protects a client is the forwarder's own
//! per-upstream deadline, which fails over regardless of what the prober
//! believes.

use arc_swap::ArcSwap;
use std::sync::Arc;
use wardnet_common::dns::UpstreamLatency;

/// Lock-free snapshot of what the latency prober last observed, one entry per
/// configured upstream.
///
/// Empty until the first probe round completes, and emptied again whenever
/// the forwarding path stops serving queries (DNS disabled, or resolving
/// recursively) — so an empty snapshot means "nothing measured", never "every
/// upstream is down".
#[derive(Debug, Default)]
pub struct UpstreamHealth {
    snapshot: ArcSwap<Vec<UpstreamLatency>>,
}

impl UpstreamHealth {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the published snapshot. Called once per probe round.
    pub fn publish(&self, latencies: Vec<UpstreamLatency>) {
        self.snapshot.store(Arc::new(latencies));
    }

    /// The current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Arc<Vec<UpstreamLatency>> {
        self.snapshot.load_full()
    }

    /// Addresses the prober currently reports as unreachable.
    ///
    /// The prober debounces before setting the flag, so an address appearing
    /// here has missed several consecutive probes rather than one packet.
    #[must_use]
    pub fn unreachable(&self) -> Vec<String> {
        self.snapshot
            .load()
            .iter()
            .filter(|u| !u.reachable)
            .map(|u| u.address.clone())
            .collect()
    }

    /// Whether `address` is currently reported unreachable.
    ///
    /// An address absent from the snapshot is *not* unreachable: it has
    /// simply not been measured yet (no probe round, or DNS is off). Treating
    /// unmeasured as unreachable would empty the serving pool on every
    /// startup.
    #[must_use]
    pub fn is_unreachable(&self, address: &str) -> bool {
        self.snapshot
            .load()
            .iter()
            .any(|u| u.address == address && !u.reachable)
    }

    /// The prober's rolling-average round-trip time for `address`, if it has
    /// produced a successful sample.
    #[must_use]
    pub fn latency_ms(&self, address: &str) -> Option<f64> {
        self.snapshot
            .load()
            .iter()
            .find(|u| u.address == address)
            .and_then(|u| u.avg_latency_ms)
    }
}
