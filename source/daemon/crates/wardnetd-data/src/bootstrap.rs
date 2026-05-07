use std::sync::Arc;

use argon2::PasswordHasher;
use argon2::password_hash::rand_core::OsRng;
use uuid::Uuid;

use crate::repository::{AdminRepository, SystemConfigRepository};

/// Ensure at least one admin account exists in the database.
///
/// Behaviour on startup:
/// 1. If an admin already exists in the database, log and return.
/// 2. If credentials are provided, create an admin with those credentials.
/// 3. Otherwise, leave the database without an admin and let the setup
///    wizard (`POST /api/setup`) create the first admin from the
///    operator's UI input. The wizard runs unauthenticated until an
///    admin exists, then locks itself out.
pub async fn bootstrap_admin(
    admin_repo: &Arc<dyn AdminRepository>,
    credentials: Option<(&str, &str)>,
) -> anyhow::Result<()> {
    if admin_repo.exists().await? {
        tracing::info!("admin account already exists, skipping bootstrap");
        return Ok(());
    }

    let Some((username, password)) = credentials else {
        tracing::info!("no admin configured, deferring to setup wizard");
        return Ok(());
    };

    let password_hash = hash_password(password)?;
    let id = Uuid::new_v4().to_string();

    admin_repo.create(&id, username, &password_hash).await?;

    tracing::info!(username = %username, "created admin from config: username={username}");

    Ok(())
}

/// Seed setup-wizard and default-policy keys in `system_config`.
///
/// Idempotent: only writes keys that are currently unset.
///
/// - `default_policy`: migrated from the operator's `config.toml`
///   (`network.default_policy`) on first boot after upgrade. The TOML
///   field is deprecated for one release; `set_default_policy` writes
///   to the DB from then on.
/// - `wizard_step`: derived from `setup_completed`. Existing installs
///   with `setup_completed = "true"` jump straight to `"completed"` so
///   they aren't forced through the new wizard.
pub async fn bootstrap_system_config(
    system_config: &Arc<dyn SystemConfigRepository>,
    config_default_policy: &str,
) -> anyhow::Result<()> {
    if system_config.get_default_policy().await?.is_none() {
        system_config
            .set_default_policy(config_default_policy)
            .await?;
        tracing::info!(
            policy = config_default_policy,
            "seeded default_policy from config.toml"
        );
    }

    if system_config.get_wizard_step().await?.is_none() {
        let initial = if system_config.is_setup_completed().await? {
            "completed"
        } else {
            "admin"
        };
        system_config.set_wizard_step(initial).await?;
        tracing::info!(step = initial, "seeded wizard_step");
    }

    Ok(())
}

/// Hash a plaintext password using Argon2id with a random salt.
fn hash_password(password: &str) -> anyhow::Result<String> {
    // Use `OsRng` from the `password_hash` crate (re-exported via `argon2`)
    // to avoid `rand_core` version conflicts between `rand 0.10` and
    // `password-hash 0.5` which depends on `rand_core 0.6`.
    let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
    let hash = argon2::Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("failed to hash password: {e}"))?;
    Ok(hash.to_string())
}
