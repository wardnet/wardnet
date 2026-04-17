use std::path::PathBuf;

use async_trait::async_trait;
use uuid::Uuid;

/// Manages `WireGuard` private key files on disk.
///
/// Keys are stored at `<keys_dir>/<tunnel-id>.key` with mode 0600.
/// Private keys must never appear in API responses, logs, or the database.
#[async_trait]
pub trait KeyStore: Send + Sync {
    /// Save a private key for the given tunnel.
    async fn save_key(&self, tunnel_id: &Uuid, private_key: &str) -> anyhow::Result<()>;

    /// Load the private key for the given tunnel.
    async fn load_key(&self, tunnel_id: &Uuid) -> anyhow::Result<String>;

    /// Delete the private key for the given tunnel.
    async fn delete_key(&self, tunnel_id: &Uuid) -> anyhow::Result<()>;
}

/// File-system backed key store.
///
/// Writes keys to `<keys_dir>/<tunnel-id>.key` with restrictive permissions.
#[derive(Debug)]
pub struct FileKeyStore {
    keys_dir: PathBuf,
}

impl FileKeyStore {
    /// Create a new file key store rooted at the given directory.
    #[must_use]
    pub fn new(keys_dir: PathBuf) -> Self {
        Self { keys_dir }
    }

    fn key_path(&self, tunnel_id: &Uuid) -> PathBuf {
        self.keys_dir.join(format!("{tunnel_id}.key"))
    }
}

#[async_trait]
impl KeyStore for FileKeyStore {
    async fn save_key(&self, tunnel_id: &Uuid, private_key: &str) -> anyhow::Result<()> {
        let path = self.key_path(tunnel_id);
        tokio::fs::create_dir_all(&self.keys_dir).await?;
        tokio::fs::write(&path, private_key.as_bytes()).await?;

        // Set file permissions to 0600 (owner read/write only).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            tokio::fs::set_permissions(&path, perms).await?;
        }

        tracing::debug!(tunnel_id = %tunnel_id, "saved private key");
        Ok(())
    }

    async fn load_key(&self, tunnel_id: &Uuid) -> anyhow::Result<String> {
        let path = self.key_path(tunnel_id);
        let key = tokio::fs::read_to_string(&path).await?;
        Ok(key)
    }

    async fn delete_key(&self, tunnel_id: &Uuid) -> anyhow::Result<()> {
        let path = self.key_path(tunnel_id);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {
                tracing::debug!(tunnel_id = %tunnel_id, "deleted private key");
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Already gone — nothing to do.
            }
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }
}
