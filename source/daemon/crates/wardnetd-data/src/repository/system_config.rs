use async_trait::async_trait;
use chrono::{DateTime, Utc};
use wardnet_common::api::LastShutdownState;

/// Snapshot of the previous daemon shutdown.
///
/// Returned both by [`SystemConfigRepository::detect_previous_shutdown`]
/// (which classifies the prior run and writes a fresh `running` marker
/// in a single transaction) and by [`SystemConfigRepository::read_last_shutdown`]
/// (which performs a read-only lookup for the API).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastShutdownInfo {
    pub state: LastShutdownState,
    pub at: Option<DateTime<Utc>>,
    pub acknowledged_at: Option<DateTime<Utc>>,
}

/// Data access for the key-value `system_config` table and aggregate counts.
///
/// Provides simple get/set for configuration values, count queries for
/// devices and tunnels, and the unclean-shutdown detection key set
/// (`last_shutdown_state`, `last_shutdown_at`, `last_shutdown_acknowledged_at`,
/// `last_seen_at`). Used by [`SystemService`](crate::service::SystemService)
/// to build status responses and by the daemon entrypoint to classify
/// the previous shutdown on boot.
#[async_trait]
pub trait SystemConfigRepository: Send + Sync {
    /// Retrieve a config value by key.
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>>;

    /// Insert or update a config value.
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()>;

    /// Remove a config value by key. No-op when the key is absent — mirrors
    /// [`SecretStore::delete`](crate::secret_store::SecretStore::delete) so a
    /// "teardown / forget" flow can truly clear a key (so [`Self::get`] returns
    /// `None`) rather than blank it to an empty string a reader might
    /// misinterpret as a configured-but-empty value.
    async fn delete(&self, key: &str) -> anyhow::Result<()>;

    /// Return the total number of rows in the `devices` table.
    async fn device_count(&self) -> anyhow::Result<i64>;

    /// Return the total number of rows in the `tunnels` table.
    async fn tunnel_count(&self) -> anyhow::Result<i64>;

    /// Return the database file size in bytes.
    async fn db_size_bytes(&self) -> anyhow::Result<u64>;

    /// Classify the previous shutdown and atomically arm the next-boot
    /// marker.
    ///
    /// Reads `last_shutdown_state`, `last_shutdown_at`, `last_seen_at`,
    /// and `last_shutdown_acknowledged_at`, then UPSERTs
    /// `last_shutdown_state="running"` and `last_shutdown_at=NOW` in
    /// the same transaction. Must be called exactly once per boot,
    /// before the heartbeat runner starts.
    ///
    /// Classification:
    /// - row absent → `Unknown`, `at = None`
    /// - prior `graceful` → `Graceful`, `at = last_shutdown_at`
    /// - prior `running` → `Unclean`, `at = last_seen_at` (falling back
    ///   to `last_shutdown_at` if the heartbeat never fired before the
    ///   crash)
    ///
    /// `acknowledged_at` is intentionally untouched by this method —
    /// the timestamp-comparison ack model (`acknowledged_at >= at`)
    /// handles the reset implicitly when a fresh unclean event writes
    /// a newer `last_shutdown_at`.
    async fn detect_previous_shutdown(&self) -> anyhow::Result<LastShutdownInfo> {
        Ok(LastShutdownInfo {
            state: LastShutdownState::Unknown,
            at: None,
            acknowledged_at: None,
        })
    }

    /// Record a graceful shutdown.
    ///
    /// Writes `last_shutdown_state="graceful"` and
    /// `last_shutdown_at=NOW`. Called from `main.rs` after the runner
    /// shutdown sequence and before the process exits.
    async fn record_graceful_shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Update the heartbeat timestamp.
    ///
    /// Writes `last_seen_at=NOW`. The heartbeat runner calls this on
    /// a 60s cadence so an unclean shutdown's `at` timestamp is
    /// accurate to within the heartbeat interval.
    async fn record_heartbeat(&self) -> anyhow::Result<()> {
        Ok(())
    }

    /// Read the current shutdown classification without modifying it.
    ///
    /// Used by `SystemService::status()` to embed the result in the
    /// dashboard's status response. Returns the same shape as
    /// [`Self::detect_previous_shutdown`] but is read-only.
    async fn read_last_shutdown(&self) -> anyhow::Result<LastShutdownInfo> {
        Ok(LastShutdownInfo {
            state: LastShutdownState::Unknown,
            at: None,
            acknowledged_at: None,
        })
    }

    /// Acknowledge the most recent unclean shutdown.
    ///
    /// Writes `last_shutdown_acknowledged_at=NOW`. Idempotent —
    /// repeated calls just refresh the timestamp. The banner predicate
    /// is `acknowledged_at < at`, so a future unclean event with a
    /// newer `last_shutdown_at` automatically makes this stale.
    async fn acknowledge_last_shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }

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

    /// Check whether new-device quarantine is enabled (issue #738).
    ///
    /// When on, a freshly-discovered device triggers an admin push to approve
    /// it. Reads the `quarantine_new_devices` key; returns `false` if the key is
    /// missing or set to any value other than `"true"` (off by default).
    async fn is_quarantine_new_devices(&self) -> anyhow::Result<bool> {
        let value = self.get("quarantine_new_devices").await?;
        Ok(value.as_deref() == Some("true"))
    }

    /// Enable or disable new-device quarantine (issue #738).
    async fn set_quarantine_new_devices(&self, enabled: bool) -> anyhow::Result<()> {
        let value = if enabled { "true" } else { "false" };
        self.set("quarantine_new_devices", value).await
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

    /// Reset the global default routing policy to `"direct"`, but only if
    /// it still holds `expected`. Returns whether the reset was applied.
    ///
    /// Used by tunnel deletion to clear a policy that points at the tunnel
    /// being removed without clobbering a concurrent
    /// [`Self::set_default_policy`] write: if the stored value no longer
    /// matches `expected`, another writer won and the reset is skipped.
    /// This default implementation is a non-atomic read-compare-write for
    /// in-memory test doubles; the `SQLite` implementation overrides it with
    /// a single conditional `UPDATE` so the compare-and-set is atomic.
    async fn reset_default_policy_from(&self, expected: &str) -> anyhow::Result<bool> {
        if self.get("default_policy").await?.as_deref() == Some(expected) {
            self.set("default_policy", "direct").await?;
            Ok(true)
        } else {
            Ok(false)
        }
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

    /// Whether the inbound-WireGuard server is enabled (issue #809).
    ///
    /// Reads the `inbound_wg_enabled` key; returns `false` if the key is missing
    /// or set to any value other than `"true"` (off by default).
    async fn inbound_wg_enabled(&self) -> anyhow::Result<bool> {
        let value = self.get("inbound_wg_enabled").await?;
        Ok(value.as_deref() == Some("true"))
    }

    /// Enable or disable the inbound-WireGuard server (issue #809).
    async fn set_inbound_wg_enabled(&self, enabled: bool) -> anyhow::Result<()> {
        let value = if enabled { "true" } else { "false" };
        self.set("inbound_wg_enabled", value).await
    }

    /// The UDP port the inbound-WireGuard server listens on (issue #809).
    ///
    /// Reads the `inbound_wg_listen_port` key; defaults to `51821` when unset or
    /// unparseable is treated as an error.
    async fn inbound_wg_listen_port(&self) -> anyhow::Result<u16> {
        self.get("inbound_wg_listen_port")
            .await?
            .unwrap_or_else(|| "51821".to_owned())
            .parse::<u16>()
            .map_err(|e| anyhow::anyhow!("invalid inbound_wg_listen_port: {e}"))
    }

    /// Persist the inbound-WireGuard listen port (issue #809).
    async fn set_inbound_wg_listen_port(&self, port: u16) -> anyhow::Result<()> {
        self.set("inbound_wg_listen_port", &port.to_string()).await
    }

    /// Whether Private DNS (the `DoT` `:853` listener) is enabled (issue #912).
    ///
    /// Reads the `private_dns_enabled` key; returns `false` if the key is
    /// missing or set to any value other than `"true"` (off by default).
    async fn private_dns_enabled(&self) -> anyhow::Result<bool> {
        let value = self.get("private_dns_enabled").await?;
        Ok(value.as_deref() == Some("true"))
    }

    /// Enable or disable Private DNS (issue #912).
    async fn set_private_dns_enabled(&self, enabled: bool) -> anyhow::Result<()> {
        let value = if enabled { "true" } else { "false" };
        self.set("private_dns_enabled", value).await
    }

    /// The inbound-WireGuard server's public key (issue #809), if generated.
    ///
    /// Stored as a plain base64 string; `None` when the server has never been
    /// enabled (no keypair exists yet).
    async fn inbound_wg_server_pubkey(&self) -> anyhow::Result<Option<String>> {
        self.get("inbound_wg_server_pubkey").await
    }

    /// Persist the inbound-WireGuard server public key (issue #809).
    async fn set_inbound_wg_server_pubkey(&self, pubkey: &str) -> anyhow::Result<()> {
        self.set("inbound_wg_server_pubkey", pubkey).await
    }
}
