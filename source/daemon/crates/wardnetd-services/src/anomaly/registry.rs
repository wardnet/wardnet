use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use wardnet_common::anomaly::AnomalyType;

use crate::anomaly::detector::AnomalyDetector;
use crate::anomaly::detectors::{
    BlocklistRefreshFailingDetector, TransientDetector, TunnelStartFailedDetector,
    TunnelUnhealthyDetector, UpdateFailedDetector,
};
use crate::dns_filter::DnsFilterService;
use crate::tunnel::TunnelService;

/// Per-detector enable/disable flags, keyed by
/// [`AnomalyType::as_str`], passed from daemon configuration.
pub type EnabledDetectors = HashMap<String, bool>;

/// Services the built-in detectors need in order to answer questions about
/// the world. Detectors talk to *services*, never repositories — the same
/// rule background runners follow.
pub struct DetectorDeps {
    pub dns_filter: Arc<dyn DnsFilterService>,
    pub tunnel: Arc<dyn TunnelService>,
    /// The running release, for `UpdateFailed`'s target comparison.
    pub running_version: String,
}

/// Registry of anomaly detectors, keyed by the type each one owns.
///
/// Built once during service wiring. Adding a detector is a registration here
/// plus a catalogue entry — nothing else in the engine changes, which is the
/// point of keeping the schedule and the lookup in one place.
pub struct AnomalyDetectorRegistry {
    detectors: HashMap<AnomalyType, Arc<dyn AnomalyDetector>>,
}

impl AnomalyDetectorRegistry {
    /// Create a registry holding every built-in detector.
    ///
    /// `enabled` maps [`AnomalyType::as_str`] slugs to flags; a type not
    /// listed is treated as enabled. Disabling one is an operational escape
    /// hatch for a detector that misbehaves on a particular box — its
    /// anomalies simply stop being raised and reevaluated.
    #[must_use]
    pub fn new(enabled: &EnabledDetectors, deps: &DetectorDeps) -> Self {
        let mut registry = Self {
            detectors: HashMap::new(),
        };

        if Self::is_enabled(enabled, AnomalyType::BlocklistRefreshFailing) {
            registry.register(Arc::new(BlocklistRefreshFailingDetector::new(
                deps.dns_filter.clone(),
            )));
        }
        if Self::is_enabled(enabled, AnomalyType::TunnelStartFailed) {
            registry.register(Arc::new(TunnelStartFailedDetector::new(
                deps.tunnel.clone(),
            )));
        }
        if Self::is_enabled(enabled, AnomalyType::TunnelUnhealthy) {
            registry.register(Arc::new(TunnelUnhealthyDetector::new(deps.tunnel.clone())));
        }
        if Self::is_enabled(enabled, AnomalyType::UpdateFailed) {
            registry.register(Arc::new(UpdateFailedDetector::new(&deps.running_version)));
        }
        if Self::is_enabled(enabled, AnomalyType::RouteTableLost) {
            registry.register(Arc::new(TransientDetector::new(
                AnomalyType::RouteTableLost,
            )));
        }
        if Self::is_enabled(enabled, AnomalyType::DhcpConflict) {
            registry.register(Arc::new(TransientDetector::new(AnomalyType::DhcpConflict)));
        }

        registry
    }

    /// An empty registry, for tests that register their own fakes.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            detectors: HashMap::new(),
        }
    }

    fn is_enabled(enabled: &EnabledDetectors, anomaly_type: AnomalyType) -> bool {
        enabled.get(anomaly_type.as_str()).copied().unwrap_or(true)
    }

    /// Register a detector, replacing any existing one for the same type.
    ///
    /// Call during wiring, before the registry is shared as an `Arc`.
    pub fn register(&mut self, detector: Arc<dyn AnomalyDetector>) {
        self.detectors.insert(detector.anomaly_type(), detector);
    }

    /// Look up the detector owning `anomaly_type`.
    ///
    /// `None` means the type is disabled or unregistered. Callers treat that
    /// as "leave it alone" rather than an error: a disabled detector's
    /// existing anomalies stay as they are instead of being force-resolved by
    /// something that no longer understands them.
    #[must_use]
    pub fn get(&self, anomaly_type: AnomalyType) -> Option<&Arc<dyn AnomalyDetector>> {
        self.detectors.get(&anomaly_type)
    }

    /// Every registered type paired with its sweep interval.
    ///
    /// Reactive-only detectors are absent — they have nothing to schedule.
    /// Sorted so the engine seeds its deadlines deterministically.
    #[must_use]
    pub fn schedule(&self) -> Vec<(AnomalyType, Duration)> {
        let mut schedule: Vec<(AnomalyType, Duration)> = self
            .detectors
            .iter()
            .filter_map(|(t, d)| d.interval().map(|i| (*t, i)))
            .collect();
        schedule.sort_by_key(|(t, _)| t.as_str());
        schedule
    }
}
