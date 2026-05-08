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

    /// Read the global default routing policy.
    ///
    /// Stored as either `"direct"` or a tunnel UUID (as plain string).
    /// Returns `None` if the key is unset (e.g. the bootstrap migration
    /// hasn't run yet).
    async fn get_default_policy(&self) -> anyhow::Result<Option<String>> {
        self.get("default_policy").await
    }

    /// Persist the global default routing policy.
    async fn set_default_policy(&self, policy: &str) -> anyhow::Result<()> {
        self.set("default_policy", policy).await
    }

    /// Read the current setup-wizard step.
    ///
    /// One of: `admin`, `network`, `dhcp`, `router_mac`, `tunnel`,
    /// `policy`, or `completed`. Returns `None` if the key is unset.
    async fn get_wizard_step(&self) -> anyhow::Result<Option<String>> {
        self.get("wizard_step").await
    }

    /// Persist the current setup-wizard step.
    async fn set_wizard_step(&self, step: &str) -> anyhow::Result<()> {
        self.set("wizard_step", step).await
    }

    /// Read the current setup-wizard mode (`primary` or `locked_router`).
    async fn get_wizard_mode(&self) -> anyhow::Result<Option<String>> {
        self.get("wizard_mode").await
    }

    /// Persist the current setup-wizard mode.
    async fn set_wizard_mode(&self, mode: &str) -> anyhow::Result<()> {
        self.set("wizard_mode", mode).await
    }

    /// Read the discovered upstream router MAC address.
    async fn get_router_mac(&self) -> anyhow::Result<Option<String>> {
        self.get("router_mac").await
    }

    /// Persist the discovered upstream router MAC address.
    async fn set_router_mac(&self, mac: &str) -> anyhow::Result<()> {
        self.set("router_mac", mac).await
    }
}
