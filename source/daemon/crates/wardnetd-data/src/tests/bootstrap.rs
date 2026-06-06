use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::bootstrap::{bootstrap_admin, bootstrap_system_config};
use crate::repository::{AdminRepository, SystemConfigRepository};

/// Mock admin repository that tracks created admins and configurable existence.
struct MockAdminRepo {
    has_admin: Mutex<bool>,
    created: Mutex<Vec<(String, String, String)>>,
}

impl MockAdminRepo {
    fn new(has_admin: bool) -> Self {
        Self {
            has_admin: Mutex::new(has_admin),
            created: Mutex::new(Vec::new()),
        }
    }

    fn created_admins(&self) -> Vec<(String, String, String)> {
        self.created.lock().unwrap().clone()
    }
}

#[async_trait]
impl AdminRepository for MockAdminRepo {
    async fn find_by_username(&self, _username: &str) -> anyhow::Result<Option<(String, String)>> {
        Ok(None)
    }
    async fn create(&self, id: &str, username: &str, password_hash: &str) -> anyhow::Result<()> {
        self.created.lock().unwrap().push((
            id.to_owned(),
            username.to_owned(),
            password_hash.to_owned(),
        ));
        Ok(())
    }
    async fn find_first_id(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn exists(&self) -> anyhow::Result<bool> {
        Ok(*self.has_admin.lock().unwrap())
    }
}

/// Helper: creates a mock repo and returns both the concrete and trait-object references.
fn mock_repo(has_admin: bool) -> (Arc<MockAdminRepo>, Arc<dyn AdminRepository>) {
    let repo = Arc::new(MockAdminRepo::new(has_admin));
    let dyn_repo: Arc<dyn AdminRepository> = repo.clone();
    (repo, dyn_repo)
}

#[tokio::test]
async fn skips_when_admin_already_exists() {
    let (repo, dyn_repo) = mock_repo(true);

    bootstrap_admin(&dyn_repo, None).await.unwrap();

    assert!(repo.created_admins().is_empty());
}

#[tokio::test]
async fn creates_admin_from_config() {
    let (repo, dyn_repo) = mock_repo(false);

    bootstrap_admin(&dyn_repo, Some(("myadmin", "mypassword")))
        .await
        .unwrap();

    let created = repo.created_admins();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].1, "myadmin");
    // Verify the stored hash is a valid argon2 hash, not the plaintext password.
    assert!(created[0].2.starts_with("$argon2"));
    // Verify the hash actually verifies against the original password.
    let parsed = argon2::PasswordHash::new(&created[0].2).unwrap();
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
    // Without `config.admin`, bootstrap leaves the database without an
    // admin so the setup wizard owns first-admin creation. A random
    // fallback would conflict with the wizard's INSERT and surface as
    // a 500 on POST /api/setup.
    let (repo, dyn_repo) = mock_repo(false);

    bootstrap_admin(&dyn_repo, None).await.unwrap();

    assert!(repo.created_admins().is_empty());
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
