//! Household user management (ADR-0031): the directory, credentials,
//! admin-issued enrolment, and federated sign-in.

pub mod ceremony;
pub mod oauth;
pub mod service;

pub use ceremony::{CEREMONY_TTL, CeremonyStore};
pub use oauth::{
    OauthClient, OauthConfig, OauthProvider, PendingOauth, ProviderEndpoints, ProviderIdentity,
    ProviderStatus, ReqwestOauthClient, ReturnTo,
};
pub use service::{
    AuthMethods, EnrolmentInvite, EnrolmentSummary, NewUser, OauthOutcome, OauthRedirect,
    UserProfile, UserService, UserServiceImpl,
};

#[cfg(test)]
mod tests;
