use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::bootstrap::{bootstrap_admin, bootstrap_system_config};
use crate::repository::SystemConfigRepository;
use crate::repository::user::{UserRepository, UserRole, UserRow};
use crate::repository::user_credential::{
    CredentialKind, CredentialLogin, CredentialRow, CredentialSummary, UserCredentialRepository,
};

/// Mock user repository with a configurable "box already claimed" answer.
struct MockUserRepo {
    has_user: Mutex<bool>,
    created: Mutex<Vec<UserRow>>,
}

impl MockUserRepo {
    fn new(has_user: bool) -> Self {
        Self {
            has_user: Mutex::new(has_user),
            created: Mutex::new(Vec::new()),
        }
    }

    fn created_users(&self) -> Vec<UserRow> {
        self.created.lock().unwrap().clone()
    }
}

#[async_trait]
impl UserRepository for MockUserRepo {
    async fn create(&self, row: &UserRow) -> anyhow::Result<()> {
        self.created.lock().unwrap().push(row.clone());
        Ok(())
    }
    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<UserRow>> {
        Ok(None)
    }
    async fn find_by_email(&self, _email: &str) -> anyhow::Result<Option<UserRow>> {
        Ok(None)
    }
    async fn find_all(&self) -> anyhow::Result<Vec<UserRow>> {
        Ok(self.created_users())
    }
    async fn update_profile(
        &self,
        _id: &str,
        _display_name: &str,
        _email: Option<&str>,
        _now: &str,
    ) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn set_enabled(&self, _id: &str, _enabled: bool, _now: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn set_role(&self, _id: &str, _role: UserRole, _now: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn delete(&self, id: &str) -> anyhow::Result<u64> {
        let mut created = self.created.lock().unwrap();
        let before = created.len();
        created.retain(|u| u.id != id);
        Ok((before - created.len()) as u64)
    }
    async fn count_enabled_admins(&self) -> anyhow::Result<i64> {
        Ok(i64::from(*self.has_user.lock().unwrap()))
    }
    async fn exists(&self) -> anyhow::Result<bool> {
        Ok(*self.has_user.lock().unwrap())
    }
    async fn find_first_enabled_admin(&self) -> anyhow::Result<Option<UserRow>> {
        Ok(self
            .created
            .lock()
            .unwrap()
            .iter()
            .find(|u| u.enabled && u.role == UserRole::Admin)
            .cloned())
    }
}

/// Mock credential repository that records the password it was handed, and can
/// be told to fail so the compensating-delete path is exercised.
struct MockCredentialRepo {
    passwords: Mutex<Vec<(String, String, String)>>,
    fail: bool,
}

impl MockCredentialRepo {
    fn new() -> Self {
        Self {
            passwords: Mutex::new(Vec::new()),
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            passwords: Mutex::new(Vec::new()),
            fail: true,
        }
    }

    /// `(user_id, subject, secret)` for each `set_password` call.
    fn set_passwords(&self) -> Vec<(String, String, String)> {
        self.passwords.lock().unwrap().clone()
    }
}

#[async_trait]
impl UserCredentialRepository for MockCredentialRepo {
    async fn insert(&self, _row: &CredentialRow) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_for_login(
        &self,
        _kind: CredentialKind,
        _subject: &str,
    ) -> anyhow::Result<Option<CredentialLogin>> {
        Ok(None)
    }
    async fn find_password(&self, _user_id: &str) -> anyhow::Result<Option<CredentialRow>> {
        Ok(None)
    }
    async fn list_for_user(&self, _user_id: &str) -> anyhow::Result<Vec<CredentialSummary>> {
        Ok(Vec::new())
    }
    async fn set_password(
        &self,
        _id: &str,
        user_id: &str,
        subject: &str,
        secret: &str,
        _now: &str,
    ) -> anyhow::Result<()> {
        if self.fail {
            anyhow::bail!("simulated credential write failure");
        }
        self.passwords.lock().unwrap().push((
            user_id.to_owned(),
            subject.to_owned(),
            secret.to_owned(),
        ));
        Ok(())
    }
    async fn touch_last_used(&self, _id: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_metadata(&self, _id: &str, _metadata: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn delete(&self, _id: &str, _user_id: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn delete_by_kind(&self, _user_id: &str, _kind: CredentialKind) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn count_by_kind(&self, _user_id: &str, _kind: CredentialKind) -> anyhow::Result<i64> {
        Ok(0)
    }
}

/// Helper: build both mocks plus their trait-object handles.
#[allow(clippy::type_complexity)]
fn mock_repos(
    has_user: bool,
) -> (
    Arc<MockUserRepo>,
    Arc<MockCredentialRepo>,
    Arc<dyn UserRepository>,
    Arc<dyn UserCredentialRepository>,
) {
    let users = Arc::new(MockUserRepo::new(has_user));
    let creds = Arc::new(MockCredentialRepo::new());
    let dyn_users: Arc<dyn UserRepository> = users.clone();
    let dyn_creds: Arc<dyn UserCredentialRepository> = creds.clone();
    (users, creds, dyn_users, dyn_creds)
}

#[tokio::test]
async fn skips_when_user_already_exists() {
    let (users, creds, dyn_users, dyn_creds) = mock_repos(true);

    bootstrap_admin(&dyn_users, &dyn_creds, None).await.unwrap();

    assert!(users.created_users().is_empty());
    assert!(creds.set_passwords().is_empty());
}

#[tokio::test]
async fn creates_admin_from_config() {
    let (users, creds, dyn_users, dyn_creds) = mock_repos(false);

    bootstrap_admin(&dyn_users, &dyn_creds, Some(("MyAdmin", "mypassword")))
        .await
        .unwrap();

    let created = users.created_users();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].display_name, "MyAdmin");
    assert_eq!(created[0].role, UserRole::Admin);
    assert!(created[0].enabled);
    // No email is invented for the config-file path — it would occupy the
    // unique email index with an address nobody can receive mail at.
    assert_eq!(created[0].email, None);

    let passwords = creds.set_passwords();
    assert_eq!(passwords.len(), 1);
    assert_eq!(
        passwords[0].0, created[0].id,
        "credential links to the user"
    );
    // Subject is lowercased so the break-glass login is case-insensitive,
    // matching how the migration backfills existing local admins.
    assert_eq!(passwords[0].1, "myadmin");
    // The stored secret is an argon2 hash, not the plaintext password.
    assert!(passwords[0].2.starts_with("$argon2"));
    let parsed = argon2::PasswordHash::new(&passwords[0].2).unwrap();
    assert!(
        argon2::PasswordVerifier::verify_password(
            &argon2::Argon2::default(),
            b"mypassword",
            &parsed,
        )
        .is_ok()
    );
}

#[tokio::test]
async fn defers_to_setup_wizard_when_no_config() {
    // Without `config.admin`, bootstrap leaves the database without a user so
    // the setup wizard owns first-admin creation. A random fallback would
    // conflict with the wizard's INSERT and surface as a 500 on POST /api/setup.
    let (users, creds, dyn_users, dyn_creds) = mock_repos(false);

    bootstrap_admin(&dyn_users, &dyn_creds, None).await.unwrap();

    assert!(users.created_users().is_empty());
    assert!(creds.set_passwords().is_empty());
}

// -- bootstrap_system_config -------------------------------------------------

struct MockSystemConfigRepo {
    store: Mutex<HashMap<String, String>>,
}

impl MockSystemConfigRepo {
    fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }

    fn with(initial: &[(&str, &str)]) -> Self {
        let mut map = HashMap::new();
        for (k, v) in initial {
            map.insert((*k).to_owned(), (*v).to_owned());
        }
        Self {
            store: Mutex::new(map),
        }
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.store.lock().unwrap().remove(key);
        Ok(())
    }

    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(0)
    }
}

#[tokio::test]
async fn bootstrap_system_config_seeds_default_policy_from_toml() {
    let repo: Arc<dyn SystemConfigRepository> = Arc::new(MockSystemConfigRepo::new());

    bootstrap_system_config(&repo, "direct").await.unwrap();

    assert_eq!(
        repo.get_default_policy().await.unwrap().as_deref(),
        Some("direct")
    );
}

#[tokio::test]
async fn bootstrap_system_config_does_not_overwrite_existing_default_policy() {
    let tunnel_uuid = "10000000-0000-0000-0000-000000000001";
    let repo: Arc<dyn SystemConfigRepository> = Arc::new(MockSystemConfigRepo::with(&[(
        "default_policy",
        tunnel_uuid,
    )]));

    // The TOML default is "direct" but the user already chose a tunnel via the
    // UI — bootstrap must not regress the persisted value to the TOML value.
    bootstrap_system_config(&repo, "direct").await.unwrap();

    assert_eq!(
        repo.get_default_policy().await.unwrap().as_deref(),
        Some(tunnel_uuid)
    );
}

#[tokio::test]
async fn bootstrap_system_config_seeds_wizard_step_admin_for_fresh_install() {
    let repo: Arc<dyn SystemConfigRepository> = Arc::new(MockSystemConfigRepo::new());

    bootstrap_system_config(&repo, "direct").await.unwrap();

    assert_eq!(
        repo.get_wizard_step().await.unwrap().as_deref(),
        Some("admin")
    );
}

#[tokio::test]
async fn bootstrap_system_config_seeds_wizard_step_completed_for_existing_install() {
    // Existing v2026.05.00 installs already finished setup_completed but
    // never recorded a wizard_step. They must NOT be forced through the new
    // wizard.
    let repo: Arc<dyn SystemConfigRepository> =
        Arc::new(MockSystemConfigRepo::with(&[("setup_completed", "true")]));

    bootstrap_system_config(&repo, "direct").await.unwrap();

    assert_eq!(
        repo.get_wizard_step().await.unwrap().as_deref(),
        Some("completed")
    );
}

#[tokio::test]
async fn bootstrap_system_config_does_not_overwrite_existing_wizard_step() {
    let repo: Arc<dyn SystemConfigRepository> =
        Arc::new(MockSystemConfigRepo::with(&[("wizard_step", "router_mac")]));

    bootstrap_system_config(&repo, "direct").await.unwrap();

    assert_eq!(
        repo.get_wizard_step().await.unwrap().as_deref(),
        Some("router_mac")
    );
}

#[tokio::test]
async fn bootstrap_system_config_seed_logs_name_the_seeded_values() {
    // On a plain-text backend the seed lines must name *which* policy and step
    // were written, not just carry them as structured fields — otherwise the
    // log reads "seeded wizard_step" with no way to tell which one.
    let capture = wardnet_test_support::capture_logs(tracing::Level::INFO);
    let repo: Arc<dyn SystemConfigRepository> = Arc::new(MockSystemConfigRepo::new());

    bootstrap_system_config(&repo, "direct").await.unwrap();

    let logs = capture.contents();
    assert!(
        logs.contains("seeded default_policy from config.toml: policy=direct"),
        "default_policy seed line must name the policy, got: {logs}"
    );
    assert!(
        logs.contains("seeded wizard_step: step=admin"),
        "wizard_step seed line must name the step, got: {logs}"
    );
}

/// A failure writing the password must not leave a half-claimed box.
///
/// Without the compensating delete, the orphan `users` row makes `exists()`
/// true forever: bootstrap is skipped on every subsequent boot, the admin has
/// no credential to log in with, and the setup wizard reads the same signal and
/// considers itself complete. That is a permanent lockout, so it is worth a
/// test rather than a comment.
#[tokio::test]
async fn a_failed_credential_write_rolls_back_the_user_row() {
    let users = Arc::new(MockUserRepo::new(false));
    let creds = Arc::new(MockCredentialRepo::failing());
    let dyn_users: Arc<dyn UserRepository> = users.clone();
    let dyn_creds: Arc<dyn UserCredentialRepository> = creds.clone();

    let result = bootstrap_admin(&dyn_users, &dyn_creds, Some(("myadmin", "mypassword"))).await;

    assert!(
        result.is_err(),
        "the failure must surface, not be swallowed"
    );
    assert!(
        users.created_users().is_empty(),
        "the orphan user row must be rolled back, or the box is permanently \
         half-claimed: exists() would stay true with no way to log in"
    );
}
