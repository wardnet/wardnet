use std::sync::Arc;

use arc_swap::ArcSwap;
use wardnet_common::egress_path::PathHealth;

/// Lock-free snapshot of what the path prober last observed, one entry per
/// egress path.
///
/// A handle rather than a method on the runner, for the reason
/// [`UpstreamHealth`](crate::dns::UpstreamHealth) is: the anomaly detector
/// registry is built during service wiring, before the daemon binary starts the
/// runner. Both sides hold a clone, so the detectors read the runner's findings
/// without an `Arc` cycle.
///
/// Empty until the first round completes. An empty snapshot means **nothing has
/// been measured**, never "every path is down" — an unmeasured path must not
/// raise an anomaly, the same rule the DNS ladder holds for an unprobed
/// upstream.
#[derive(Debug, Default)]
pub struct EgressPathHealth {
    snapshot: ArcSwap<Vec<PathHealth>>,
}

impl EgressPathHealth {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the published snapshot. Called once per probe round.
    pub fn publish(&self, paths: Vec<PathHealth>) {
        self.snapshot.store(Arc::new(paths));
    }

    /// The current snapshot.
    #[must_use]
    pub fn snapshot(&self) -> Arc<Vec<PathHealth>> {
        self.snapshot.load_full()
    }

    /// The entry for one path, if it was in the last round.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<PathHealth> {
        self.snapshot
            .load()
            .iter()
            .find(|p| p.path == path)
            .cloned()
    }
}
