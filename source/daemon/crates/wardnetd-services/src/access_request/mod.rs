pub mod approver;
pub mod service;

pub use approver::{AccessRequestApprover, ApproverRegistry, PrivateDnsApprover};
pub use service::{AccessRequestService, AccessRequestServiceImpl};

#[cfg(test)]
mod tests;
