//! Household user management (ADR-0031): the directory, credentials,
//! admin-issued enrolment, and federated sign-in.

pub mod ceremony;
pub mod oauth;
pub mod passkey;
pub mod service;

pub use ceremony::{CEREMONY_TTL, CeremonyStore};
pub use oauth::{
    OauthClient, OauthConfig, OauthProvider, PendingOauth, ProviderEndpoints, ProviderIdentity,
    ProviderStatus, ReqwestOauthClient,
};
pub use passkey::{
    KEY_PASSKEY_RP_ID, PasskeyMetadata, PasskeyRelyingParty, PasskeyUnavailable,
    PendingAuthentication, PendingRegistration,
};
pub use service::{
    EnrolmentInvite, EnrolmentSummary, NewUser, UserProfile, UserService, UserServiceImpl,
};

#[cfg(test)]
mod tests;
