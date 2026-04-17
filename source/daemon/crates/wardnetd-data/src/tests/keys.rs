use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use uuid::Uuid;

use crate::keys::KeyStore;

/// In-memory key store used exclusively in tests.
#[derive(Debug, Default)]
struct InMemoryKeyStore {
    keys: Mutex<HashMap<Uuid, String>>,
}

impl InMemoryKeyStore {
    fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl KeyStore for InMemoryKeyStore {
    async fn save_key(&self, tunnel_id: &Uuid, private_key: &str) -> anyhow::Result<()> {
        self.keys
            .lock()
            .expect("lock poisoned")
            .insert(*tunnel_id, private_key.to_owned());
        Ok(())
    }

    async fn load_key(&self, tunnel_id: &Uuid) -> anyhow::Result<String> {
        self.keys
            .lock()
            .expect("lock poisoned")
            .get(tunnel_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("key not found for tunnel {tunnel_id}"))
    }

    async fn delete_key(&self, tunnel_id: &Uuid) -> anyhow::Result<()> {
        self.keys.lock().expect("lock poisoned").remove(tunnel_id);
        Ok(())
    }
}

#[tokio::test]
async fn save_and_load_key() {
    let store = InMemoryKeyStore::new();
    let id = Uuid::new_v4();
    let key = "dGVzdC1wcml2YXRlLWtleS1iYXNlNjQ=";

    store.save_key(&id, key).await.expect("save should succeed");
    let loaded = store.load_key(&id).await.expect("load should succeed");

    assert_eq!(loaded, key);
}

#[tokio::test]
async fn load_missing_key_returns_error() {
    let store = InMemoryKeyStore::new();
    let id = Uuid::new_v4();

    let result = store.load_key(&id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn delete_key_removes_it() {
    let store = InMemoryKeyStore::new();
    let id = Uuid::new_v4();
    let key = "dGVzdC1wcml2YXRlLWtleS1iYXNlNjQ=";

    store.save_key(&id, key).await.expect("save should succeed");
    store.delete_key(&id).await.expect("delete should succeed");

    let result = store.load_key(&id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn delete_nonexistent_key_is_ok() {
    let store = InMemoryKeyStore::new();
    let id = Uuid::new_v4();

    // Should not error.
    store
        .delete_key(&id)
        .await
        .expect("delete of missing key should succeed");
}

#[tokio::test]
async fn file_key_store_round_trip() {
    use crate::keys::FileKeyStore;

    let dir = std::env::temp_dir().join(format!("wardnet-test-keys-{}", Uuid::new_v4()));
    let store = FileKeyStore::new(dir.clone());
    let id = Uuid::new_v4();
    let key = "dGVzdC1wcml2YXRlLWtleS1iYXNlNjQ=";

    store.save_key(&id, key).await.expect("save should succeed");
    let loaded = store.load_key(&id).await.expect("load should succeed");
    assert_eq!(loaded, key);

    // Verify restrictive permissions on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(format!("{id}.key"));
        let meta = tokio::fs::metadata(&path).await.expect("file should exist");
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    store.delete_key(&id).await.expect("delete should succeed");
    assert!(store.load_key(&id).await.is_err());

    // Clean up.
    let _ = tokio::fs::remove_dir_all(&dir).await;
}
