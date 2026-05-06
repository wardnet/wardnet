//! Per-device filter context.
//!
//! Owns the device's kill-switch flag and the set of profiles its queries
//! evaluate against. The service layer materialises one of these per IP
//! and refreshes them on `DeviceIpChanged` / device-assignment events.

use std::net::IpAddr;
use std::sync::Arc;

use arc_swap::ArcSwap;
use hickory_proto::rr::RecordType;
use wardnet_common::dns::FilterAction;

use super::filter::MatchKind;
use super::filter_profile::{ProfileMatch, RuntimeDnsFilterProfile};

/// What the runtime knows about one device's filtering.
#[derive(Debug)]
pub struct DeviceFilterContext {
    /// Per-device kill switch. `false` = bypass filtering for this device.
    pub enabled: bool,
    /// Explicit profile assignments. Empty means the runtime substitutes the
    /// default-profile context.
    pub profiles: Vec<Arc<ArcSwap<RuntimeDnsFilterProfile>>>,
}

impl DeviceFilterContext {
    #[must_use]
    pub fn unfiltered() -> Self {
        Self {
            enabled: true,
            profiles: Vec::new(),
        }
    }

    /// Evaluate this device's profiles. Returns the would-be filter action
    /// — the caller is responsible for honouring `enabled` (the kill
    /// switch) and the global emergency stop.
    #[must_use]
    pub fn check(&self, domain: &str, qtype: RecordType, client: IpAddr) -> FilterAction {
        let mut best: Option<MatchKind> = None;
        for profile in &self.profiles {
            let snapshot = profile.load();
            match snapshot.check(domain, qtype, client) {
                ProfileMatch::Rewrite { ip } => return FilterAction::Rewrite { ip },
                ProfileMatch::Decided(kind) => {
                    if best.is_none_or(|c| kind.rank() > c.rank()) {
                        best = Some(kind);
                    }
                }
                ProfileMatch::None => {}
            }
        }
        match best {
            Some(k) if k.blocks() => FilterAction::Block,
            _ => FilterAction::Pass,
        }
    }
}
