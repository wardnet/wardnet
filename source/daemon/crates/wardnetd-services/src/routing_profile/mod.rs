//! Routing profiles: per-domain routing rules grouped into named profiles and
//! assigned to devices in priority order. The routing sibling of the DNS filter
//! profile subsystem — see [`service`] for the design.

pub mod runner;
pub mod service;
pub mod view;

#[cfg(test)]
mod tests;

pub use runner::DomainRouteRunner;
pub use service::{DomainRouteRequest, RoutingProfileService, RoutingProfileServiceImpl};
pub use view::{DeviceRoutingContext, RoutingView};
