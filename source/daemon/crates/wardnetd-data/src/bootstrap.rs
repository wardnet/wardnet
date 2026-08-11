use std::sync::Arc;

use argon2::PasswordHasher;
use argon2::password_hash::rand_core::OsRng;
use uuid::Uuid;

use crate::repository::SystemConfigRepository;
use crate::repository::user::{UserRepository, UserRole, UserRow};
use crate::repository::user_credential::UserCredentialRepository;

/// Ensure at least one household user exists in the database.
///
/// Behaviour on startup:
/// 1. If any user already exists in the database, log and return.
/// 2. If credentials are provided, create an `admin`-role user with a
///    `password` credential.
/// 3. Otherwise, leave the database empty and let the setup wizard
///    (`POST /api/setup`) create the first admin from the operator's UI input.
///    The wizard runs unauthenticated until a user exists, then locks itself
///    out.
///
/// The two rows are written separately and this repository layer exposes no
/// transaction, so a failure **between** them has to be handled explicitly. It
/// is not benign: `exists()` in step 1 asks whether any `users` row is present,
/// so a box left holding a user with no credential would skip bootstrap
/// forever, be unable to log in, and — because `bootstrap_system_config` reads
/// the same signal — consider its setup wizard complete. That is a permanent
/// lockout with no recovery path short of editing the database by hand.
///
/// So the credential failure path **compensates**: the orphan `users` row is
/// deleted before the error is returned, leaving the box exactly as unclaimed
/// as it was, and the next boot retries cleanly. If that cleanup itself fails
/// the original error still surfaces, with the cleanup failure logged — there
/// is nothing further this layer can honestly do, and the operator needs to
/// see both.
pub async fn bootstrap_admin(
    user_repo: &Arc<dyn UserRepository>,
    credential_repo: &Arc<dyn UserCredentialRepository>,
    credentials: Option<(&str, &str)>,
) -> anyhow::Result<()> {
    if user_repo.exists().await? {
        tracing::info!("household user already exists, skipping bootstrap");
        return Ok(());
    }

    let Some((username, password)) = credentials else {
        tracing::info!("no admin configured, deferring to setup wizard");
        return Ok(());
    };

    let password_hash = hash_password(password)?;
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    user_repo
        .create(&UserRow {
            id: id.clone(),
            display_name: username.to_owned(),
            // No email: the config-file bootstrap path only knows a username,
            // and inventing `username@localhost` would occupy the unique email
            // index with a value nobody can receive mail at.
            email: None,
            role: UserRole::Admin,
            enabled: true,
            created_at: now.clone(),
            updated_at: now.clone(),
        })
        .await?;

    let credential = credential_repo
        .set_password(
            &Uuid::new_v4().to_string(),
            &id,
            // Lowercased so the break-glass login is case-insensitive, matching
            // how the migration backfills existing local admins.
            &username.to_lowercase(),
            &password_hash,
            &now,
        )
        .await;

    if let Err(e) = credential {
        // Compensate: without this the box is permanently half-claimed. See the
        // function docs.
        if let Err(cleanup) = user_repo.delete(&id).await {
            tracing::error!(
                error = %cleanup,
                user_id = %id,
                "failed to roll back the half-created admin; the box may be \
                 left with a credential-less user that blocks the setup wizard"
            );
        }
        return Err(e);
    }

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
            "seeded default_policy from config.toml: policy={config_default_policy}"
        );
    }

    if system_config.get_wizard_step().await?.is_none() {
        let initial = if system_config.is_setup_completed().await? {
            "completed"
        } else {
            "admin"
        };
        system_config.set_wizard_step(initial).await?;
        tracing::info!(step = initial, "seeded wizard_step: step={initial}");
    }

    Ok(())
}

/// Hash a plaintext password using `Argon2id` with a random salt.
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
