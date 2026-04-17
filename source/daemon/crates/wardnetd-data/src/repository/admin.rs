use async_trait::async_trait;

/// Data access for admin accounts.
///
/// Provides CRUD operations for the `admins` table. No business logic —
/// validation, password hashing, and policy checks belong in the service layer.
#[async_trait]
pub trait AdminRepository: Send + Sync {
    /// Look up an admin by username, returning `(id, password_hash)` if found.
    async fn find_by_username(&self, username: &str) -> anyhow::Result<Option<(String, String)>>;

    /// Insert a new admin row.
    async fn create(&self, id: &str, username: &str, password_hash: &str) -> anyhow::Result<()>;

    /// Return the id of the first admin (used for single-admin MVP).
    async fn find_first_id(&self) -> anyhow::Result<Option<String>>;

    /// Return `true` if at least one admin account exists.
    async fn exists(&self) -> anyhow::Result<bool>;
}
