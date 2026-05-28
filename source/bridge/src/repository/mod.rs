pub mod challenge;
pub mod install;

pub use challenge::{ChallengeRepository, RegistrationChallenge, SqliteChallengeRepository};
pub use install::{Install, InstallRepository, SqliteInstallRepository};

#[cfg(test)]
mod tests;
