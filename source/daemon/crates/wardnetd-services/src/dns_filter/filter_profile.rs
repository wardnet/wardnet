//! Runtime DNS filter profile — a named bundle of [`DnsFilter`] sources.
//!
//! Each profile owns the per-source filters compiled for it; a profile's
//! [`check`](Self::check) iterates its filters and aggregates outcomes via
//! the natural [`MatchKind`] precedence.

use std::net::IpAddr;
use std::sync::Arc;

use arc_swap::ArcSwap;
use hickory_proto::rr::RecordType;
use uuid::Uuid;

use super::filter::{DnsFilter, MatchKind, MatchOutcome};

/// A profile snapshot the runtime points at. Filters are held behind
/// `ArcSwap` so a single source rebuild swaps in place without touching
/// the profile.
#[derive(Debug)]
pub struct RuntimeDnsFilterProfile {
    pub id: Uuid,
    pub name: String,
    pub filters: Vec<Arc<ArcSwap<DnsFilter>>>,
}

impl RuntimeDnsFilterProfile {
    #[must_use]
    pub fn new(id: Uuid, name: String, filters: Vec<Arc<ArcSwap<DnsFilter>>>) -> Self {
        Self { id, name, filters }
    }

    /// Aggregate outcomes across every filter in this profile.
    /// Returns `None` if no filter matched, otherwise the strongest
    /// [`MatchKind`] (or a `Rewrite` short-circuit).
    #[must_use]
    pub fn check(&self, domain: &str, qtype: RecordType, client: IpAddr) -> ProfileMatch {
        let mut best: Option<MatchKind> = None;
        for filter in &self.filters {
            let snapshot = filter.load();
            match snapshot.check(domain, qtype, client) {
                MatchOutcome::Rewrite { ip } => return ProfileMatch::Rewrite { ip },
                MatchOutcome::Match(kind) => {
                    if best.is_none_or(|c| kind.rank() > c.rank()) {
                        best = Some(kind);
                    }
                }
                MatchOutcome::None => {}
            }
        }
        match best {
            Some(k) => ProfileMatch::Decided(k),
            None => ProfileMatch::None,
        }
    }
}

/// Aggregated outcome of one profile's filters against a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileMatch {
    None,
    Decided(MatchKind),
    Rewrite { ip: IpAddr },
}
