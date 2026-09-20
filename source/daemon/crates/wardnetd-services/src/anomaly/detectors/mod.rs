//! The detectors behind each [`wardnet_common::anomaly::AnomalyType`].

pub mod blocklist_refresh;
pub mod dns_upstream;
pub mod transient;
pub mod tunnel;
pub mod update;

pub use blocklist_refresh::BlocklistRefreshFailingDetector;
pub use dns_upstream::DnsUpstreamUnreachableDetector;
pub use transient::TransientDetector;
pub use tunnel::{TunnelStartFailedDetector, TunnelUnhealthyDetector};
pub use update::UpdateFailedDetector;
