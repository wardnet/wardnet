//! Per-service clients for the **wardnet-cloud** control plane.
//!
//! wardnet-cloud is a service mesh — `tenants` (global identity/account
//! authority), `ddns` and `tunneller` (regional), with `billing`/`subscriptions`
//! internal to `tenants`. The daemon consumes `tenants` and `ddns`; each gets its
//! **own** client with an **independently configured endpoint** so the current
//! co-location (and any future split onto separate hosts) is a config change, not
//! a code change.
//!
//! Shared concerns live here:
//!
//! * [`pop`] — the Ed25519 proof-of-possession signature on every authenticated
//!   request;
//! * [`identity::DaemonIdentity`] — the daemon's key + cached identity JWT +
//!   entitlement flag;
//! * [`request`] — request building (PoP/bearer), status classification, JSON
//!   decoding.
//!
//! The clients ([`TenantsClient`], [`DdnsClient`]) and the wardnet-managed
//! [`WardnetDnsProvider`] sit on top. The `tunneller` client is intentionally
//! absent (reverse-tunnel is out of scope).

pub mod ddns;
pub mod identity;
pub mod pop;
pub(crate) mod request;
pub mod tenants;

#[cfg(test)]
mod tests;

pub use ddns::{DdnsClient, WardnetDnsProvider};
pub use identity::DaemonIdentity;
pub use tenants::{NetworkRegistration, TenantsClient};

use thiserror::Error;

/// An error from a wardnet-cloud control-plane call.
#[derive(Debug, Error)]
pub enum CloudError {
    /// Token minting was refused with `403` — the tenant's subscription is not
    /// active. The daemon is **suspended**: re-minting will keep failing until
    /// the account resubscribes. Distinct from a transient failure so the
    /// caller can drive the Suspended state rather than retry blindly.
    #[error("tenant subscription is not active")]
    EntitlementLost,
    /// A caller-fixable rejection (4xx) — bad enrollment code, taken/invalid
    /// slug, malformed input. Carries the upstream detail for the operator.
    #[error("{0}")]
    BadRequest(String),
    /// A transport failure or an unexpected (5xx / non-JSON) upstream response.
    #[error(transparent)]
    Upstream(#[from] anyhow::Error),
}
