use std::sync::Arc;

use argon2::PasswordHasher;
use argon2::password_hash::rand_core::OsRng;
use rand::RngExt;
use uuid::Uuid;

use crate::repository::AdminRepository;

/// Ensure at least one admin account exists in the database.
///
/// Behaviour on startup:
/// 1. If an admin already exists in the database, log and return.
/// 2. If credentials are provided, create an admin with those credentials.
/// 3. Otherwise, generate a random 16-character password for a default "admin"
///    user and log the credentials so the operator can retrieve them.
pub async fn bootstrap_admin(
    admin_repo: &Arc<dyn AdminRepository>,
    credentials: Option<(&str, &str)>,
) -> anyhow::Result<()> {
    if admin_repo.exists().await? {
        tracing::info!("admin account already exists, skipping bootstrap");
        return Ok(());
    }

    let (username, password) = if let Some((u, p)) = credentials {
        (u.to_owned(), p.to_owned())
    } else {
        let password = generate_random_password(16);
        ("admin".to_owned(), password)
    };

    let password_hash = hash_password(&password)?;
    let id = Uuid::new_v4().to_string();

    admin_repo.create(&id, &username, &password_hash).await?;

    if credentials.is_some() {
        tracing::info!(username = %username, "created admin from config: username={username}");
    } else {
        tracing::warn!(
            username = %username,
            password = %password,
            "no admin found, created default: username={username}, password={password}"
        );
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

/// Generate a random alphanumeric password of the given length.
fn generate_random_password(len: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    (0..len)
        .map(|_| {
            let idx = rng.random_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
