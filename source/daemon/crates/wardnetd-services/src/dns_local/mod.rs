//! Authoritative local-DNS CRUD service (issue #217).
//!
//! Owns the user-facing CRUD over the three local-DNS resources seeded by the
//! C1 data layer ([`wardnetd_data::repository::DnsLocalRepository`]):
//! authoritative **zones**, custom **records**, and **forwarding rules**
//! (conditional forwarding: `domain → upstream` overrides). It sits parallel
//! to [`crate::dns_filter::DnsFilterService`] — the DNS *server lifecycle*
//! stays in [`crate::dns::DnsService`].
//!
//! Scope is deliberately CRUD-only: resolver lookup (`find_records_by_domain`)
//! and upsert-by-`(domain, record_type)` are later chunks of #217.

pub mod service;

pub use service::{DnsLocalService, DnsLocalServiceImpl};

#[cfg(test)]
mod tests;
