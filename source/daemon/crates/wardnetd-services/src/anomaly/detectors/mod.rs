//! The detectors behind each [`wardnet_common::anomaly::AnomalyType`].

pub mod blocklist_refresh;
pub mod transient;
pub mod tunnel;
pub mod update;

pub use blocklist_refresh::BlocklistRefreshFailingDetector;
pub use transient::TransientDetector;
pub use tunnel::{TunnelStartFailedDetector, TunnelUnhealthyDetector};
pub use update::UpdateFailedDetector;
