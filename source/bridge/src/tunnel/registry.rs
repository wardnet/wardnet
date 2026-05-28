use dashmap::DashMap;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

/// An inbound TCP connection that the SNI demuxer wants forwarded to a Pi.
pub struct ForwardRequest {
    /// The accepted TCP stream (TLS `ClientHello` still in the buffer).
    pub stream: TcpStream,
    /// Destination port the Pi should connect to locally (443 or 853).
    pub dest_port: u16,
}

/// Thread-safe map from install slug → active tunnel sender.
///
/// When a Pi opens a WebSocket tunnel, its slug is registered here.
/// The SNI demuxer uses [`TunnelRegistry::forward`] to hand inbound
/// connections to the right tunnel handler.
pub struct TunnelRegistry {
    /// slug → sender for [`ForwardRequest`]s
    by_name: DashMap<String, mpsc::Sender<ForwardRequest>>,
    /// `install_id` → slug, for efficient cleanup on disconnect
    by_id: DashMap<String, String>,
}

impl Default for TunnelRegistry {
    fn default() -> Self {
        Self {
            by_name: DashMap::new(),
            by_id: DashMap::new(),
        }
    }
}

impl TunnelRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a Pi tunnel and return the receiver for inbound connections.
    ///
    /// If a previous registration exists for the same slug it is silently
    /// replaced (the old sender is dropped, closing the previous channel).
    #[must_use]
    pub fn register(&self, install_id: &str, name: &str) -> mpsc::Receiver<ForwardRequest> {
        let (tx, rx) = mpsc::channel(16);
        self.by_name.insert(name.to_string(), tx);
        self.by_id.insert(install_id.to_string(), name.to_string());
        rx
    }

    /// Remove a Pi tunnel registration by install ID.
    pub fn unregister(&self, install_id: &str) {
        if let Some((_, name)) = self.by_id.remove(install_id) {
            self.by_name.remove(&name);
        }
    }

    /// Forward an inbound connection to the Pi registered under `name`.
    ///
    /// Returns `true` when the forward was accepted, `false` when no tunnel
    /// is registered for that name or the tunnel's buffer is full.
    pub async fn forward(&self, name: &str, req: ForwardRequest) -> bool {
        // Clone the sender while the DashMap ref is held, then drop it before await.
        let tx = self.by_name.get(name).map(|r| r.value().clone());
        match tx {
            Some(tx) => tx.send(req).await.is_ok(),
            None => false,
        }
    }

    /// Return `true` when a tunnel is currently registered for `name`.
    #[must_use]
    pub fn is_connected(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }
}

#[cfg(test)]
mod tests;
