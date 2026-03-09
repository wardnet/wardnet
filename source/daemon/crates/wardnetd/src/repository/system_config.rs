use async_trait::async_trait;

/// Data access for the key-value `system_config` table and aggregate counts.
///
/// Provides simple get/set for configuration values and count queries for
/// devices and tunnels. Used by [`SystemService`](crate::service::SystemService)
/// to build status responses.
#[async_trait]
pub trait SystemConfigRepository: Send + Sync {
    /// Retrieve a config value by key.
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>>;

    /// Insert or update a config value.
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()>;

    /// Return the total number of rows in the `devices` table.
    async fn device_count(&self) -> anyhow::Result<i64>;

    /// Return the total number of rows in the `tunnels` table.
    async fn tunnel_count(&self) -> anyhow::Result<i64>;

    /// Return the database file size in bytes.
    async fn db_size_bytes(&self) -> anyhow::Result<u64>;

    /// Check whether the initial setup wizard has been completed.
    ///
    /// Reads the `setup_completed` key from `system_config`. Returns `false`
    /// if the key is missing or set to any value other than `"true"`.
    async fn is_setup_completed(&self) -> anyhow::Result<bool> {
        let value = self.get("setup_completed").await?;
        Ok(value.as_deref() == Some("true"))
    }

    /// Mark the setup wizard as completed (or not).
    async fn set_setup_completed(&self, completed: bool) -> anyhow::Result<()> {
        let value = if completed { "true" } else { "false" };
        self.set("setup_completed", value).await
    }
}
