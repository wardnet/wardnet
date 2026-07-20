//! Compiled, hot-path routing view.
//!
//! The service compiles every device's ordered profiles into a
//! [`DeviceRoutingContext`] and publishes the set as a [`RoutingView`] keyed by
//! device id. The DNS server, after resolving a query, looks up the querying
//! device's context and asks it which routing target (if any) the queried name
//! maps to.

use std::collections::HashMap;
use std::sync::Arc;

use uuid::Uuid;
use wardnet_common::routing_profile::{DomainRoutingTarget, domain_matches, pattern_specificity};

/// One enabled rule, compiled for matching.
pub struct CompiledRule {
    pub pattern: String,
    pub target: DomainRoutingTarget,
}

/// A device's ordered routing profiles, ready for matching.
#[derive(Default)]
pub struct DeviceRoutingContext {
    /// The device's profiles in priority order (index 0 = highest priority);
    /// each inner vec is that profile's enabled rules.
    pub profiles: Vec<Vec<CompiledRule>>,
}

impl DeviceRoutingContext {
    /// Resolve `name` to a routing target, or `None` if no assigned profile
    /// matches. Profiles are consulted in priority order and the **first**
    /// profile with a matching rule decides — a lower-priority profile can
    /// never override a higher one. Within that winning profile the
    /// most-specific matching pattern is used.
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<&DomainRoutingTarget> {
        for profile in &self.profiles {
            let best = profile
                .iter()
                .filter(|rule| domain_matches(&rule.pattern, name))
                .max_by_key(|rule| pattern_specificity(&rule.pattern));
            if let Some(rule) = best {
                return Some(&rule.target);
            }
        }
        None
    }
}

/// The published routing view: every device with at least one assigned profile,
/// keyed by device id.
pub type RoutingView = HashMap<Uuid, Arc<DeviceRoutingContext>>;
