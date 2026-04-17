use async_trait::async_trait;

/// Data access for API keys.
///
/// Stores argon2-hashed API keys. Listing returns hashes so the service layer
/// can verify incoming keys; this repository never sees plaintext keys.
#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    /// Return all `(id, key_hash)` pairs for verification.
    async fn find_all_hashes(&self) -> anyhow::Result<Vec<(String, String)>>;

    /// Insert a new API key row.
    async fn create(
        &self,
        id: &str,
        label: &str,
        key_hash: &str,
        created_at: &str,
    ) -> anyhow::Result<()>;

    /// Update the `last_used_at` timestamp for the given key.
    async fn update_last_used(&self, id: &str, now: &str) -> anyhow::Result<()>;
}
