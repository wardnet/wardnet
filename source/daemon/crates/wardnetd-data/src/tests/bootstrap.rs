use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::bootstrap::bootstrap_admin;
use crate::repository::AdminRepository;

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
