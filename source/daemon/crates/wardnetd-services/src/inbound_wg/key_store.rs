//! Inbound-`WireGuard` server key-store facade (issue #809).
//!
//! The daemon stores the inbound server's private key in the general-purpose
//! [`SecretStore`] alongside every other secret. `InboundWgService` never talks
//! to the secret store directly — it goes through the narrower
//! [`ServerKeyStore`] trait defined here, which exposes only the operations the
//! inbound server cares about.
//!
//! Unlike the outbound [`KeyStore`](crate::tunnel::KeyStore), the inbound server
//! has exactly **one** keypair (it is a singleton, not per-tunnel), so there is
//! no per-id parameter: the key lives at a fixed path.
//!
//! Keeping the trait + adapter local to this module means the rest of the
//! codebase only ever sees `SecretStore` — nothing can accidentally couple to
//! the narrower interface.

use std::sync::Arc;

use async_trait::async_trait;
use wardnetd_data::secret_store::SecretStore;

/// Fixed path of the inbound server private key inside the secret store.
const SERVER_KEY_PATH: &str = "wireguard-inbound/server.key";

/// Narrow interface for reading and writing the inbound server private key.
///
/// The private key never appears in API responses, logs, or the database.
#[async_trait]
pub trait ServerKeyStore: Send + Sync {
    /// Save the server private key.
    async fn save_key(&self, private_key: &str) -> anyhow::Result<()>;

    /// Load the server private key, if one has been generated.
    async fn load_key(&self) -> anyhow::Result<Option<String>>;

    /// Delete the server private key.
    async fn delete_key(&self) -> anyhow::Result<()>;
}

/// Adapts a [`SecretStore`] to the narrower [`ServerKeyStore`] interface.
///
/// The key is stored at `wireguard-inbound/server.key` under the store root.
pub struct ServerKeyStoreAdapter {
    store: Arc<dyn SecretStore>,
}

impl ServerKeyStoreAdapter {
    /// Wrap a shared [`SecretStore`] as a [`ServerKeyStore`].
    #[must_use]
    pub fn new(store: Arc<dyn SecretStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl ServerKeyStore for ServerKeyStoreAdapter {
    async fn save_key(&self, private_key: &str) -> anyhow::Result<()> {
        self.store
            .put(SERVER_KEY_PATH, private_key.as_bytes())
            .await?;
        tracing::debug!("saved inbound wireguard server private key");
        Ok(())
    }

    async fn load_key(&self) -> anyhow::Result<Option<String>> {
        let Some(bytes) = self.store.get(SERVER_KEY_PATH).await? else {
            return Ok(None);
        };
        let key = String::from_utf8(bytes)
            .map_err(|e| anyhow::anyhow!("inbound server private key is not valid utf-8: {e}"))?;
        Ok(Some(key))
    }

    async fn delete_key(&self) -> anyhow::Result<()> {
        self.store.delete(SERVER_KEY_PATH).await?;
        tracing::debug!("deleted inbound wireguard server private key");
        Ok(())
    }
}
