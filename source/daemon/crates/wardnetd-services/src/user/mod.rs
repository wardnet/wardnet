//! Household user management (ADR-0031): the directory, credentials, and
//! admin-issued enrolment.

pub mod service;

pub use service::{
    EnrolmentInvite, EnrolmentSummary, NewUser, UserProfile, UserService, UserServiceImpl,
};

#[cfg(test)]
mod tests;
