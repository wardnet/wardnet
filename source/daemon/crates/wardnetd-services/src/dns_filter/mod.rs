//! DNS filtering subsystem (issue #221).
//!
//! Splits the original adblock subsystem into a multi-level pipeline:
//! per-source [`DnsFilter`] → per-profile [`RuntimeDnsFilterProfile`] →
//! per-device [`DeviceFilterContext`] → [`DnsFilterService`].
//!
//! The DNS server hot path holds a single
//! [`Arc<dyn DnsFilterService>`] and calls [`DnsFilterService::check`] —
//! global emergency stop, per-device kill switch, default-profile
//! fallback and rule aggregation are all handled inside the service.

pub mod blocklist_downloader;
pub mod device_context;
pub mod filter;
pub mod filter_profile;
pub mod runner;
pub mod service;

pub use blocklist_downloader::{BlocklistFetcher, HttpBlocklistFetcher};
pub use device_context::DeviceFilterContext;
pub use filter::{DnsFilter, DnsFilterInputs, FilterStats};
pub use filter_profile::RuntimeDnsFilterProfile;
pub use runner::DnsFilterRunner;
pub use service::{DnsFilterService, DnsFilterServiceImpl};
