pub mod challenge;
pub mod install;

pub use challenge::{ChallengeRepository, MySqlChallengeRepository, RegistrationChallenge};
pub use install::{Install, InstallRepository, MySqlInstallRepository};

#[cfg(test)]
mod tests;
