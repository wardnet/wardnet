//! In-memory [`KeyStore`] implementation for the mock server.

use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnetd_data::keys::KeyStore;

/// A [`KeyStore`] that stores private key material in memory only.
///
/// Keys disappear when the process exits. This is intentional for the mock:
/// seeded tunnels do not have real keys, and any admin-uploaded keys during
/// a dev session should not persist.
#[derive(Debug, Default)]
pub struct InMemoryKeyStore {
    keys: Mutex<HashMap<Uuid, String>>,
}

#[async_trait]
impl KeyStore for InMemoryKeyStore {
    async fn save_key(&self, tunnel_id: &Uuid, private_key: &str) -> anyhow::Result<()> {
        let mut guard = self.keys.lock().await;
        guard.insert(*tunnel_id, private_key.to_owned());
        tracing::debug!(
            tunnel_id = %tunnel_id,
            "mock key_store save_key: tunnel_id={tunnel_id}",
        );
        Ok(())
    }

    async fn load_key(&self, tunnel_id: &Uuid) -> anyhow::Result<String> {
        let guard = self.keys.lock().await;
        guard.get(tunnel_id).cloned().ok_or_else(|| {
            anyhow::anyhow!("mock key_store: key not found for tunnel_id={tunnel_id}")
        })
    }

    async fn delete_key(&self, tunnel_id: &Uuid) -> anyhow::Result<()> {
        let mut guard = self.keys.lock().await;
        guard.remove(tunnel_id);
        tracing::debug!(
            tunnel_id = %tunnel_id,
            "mock key_store delete_key: tunnel_id={tunnel_id}",
        );
        Ok(())
    }
}
