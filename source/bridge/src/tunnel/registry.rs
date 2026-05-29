use dashmap::DashMap;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

/// An inbound TLS connection handed off from the SNI demuxer to a Pi tunnel handler.
pub struct ForwardRequest {
    /// The accepted TCP stream (TLS `ClientHello` still in the buffer).
    pub stream: TcpStream,
    /// Destination port the Pi should connect to locally (443 or 853).
    pub dest_port: u16,
}

/// Result of attempting to forward an inbound connection to a registered tunnel.
pub enum ForwardResult {
    /// The connection was accepted and queued for the Pi.
    Accepted,
    /// No tunnel is registered for the requested install slug.
    NotConnected,
    /// A tunnel is registered but its receive buffer is full; the connection was dropped.
    BufferFull,
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
        let (tx, rx) = mpsc::channel(64);
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
    /// Returns [`ForwardResult::Accepted`] when the connection was queued,
    /// [`ForwardResult::NotConnected`] when no tunnel is registered for `name`,
    /// or [`ForwardResult::BufferFull`] when the tunnel's receive buffer is full.
    /// Uses a non-blocking send so the SNI accept loop is never stalled by a
    /// slow or unresponsive Pi.
    pub fn forward(&self, name: &str, req: ForwardRequest) -> ForwardResult {
        // Clone the sender while the DashMap ref is held, then drop it before the send.
        let tx = self.by_name.get(name).map(|r| r.value().clone());
        match tx {
            None => ForwardResult::NotConnected,
            Some(tx) => match tx.try_send(req) {
                Ok(()) => ForwardResult::Accepted,
                Err(TrySendError::Full(_)) => ForwardResult::BufferFull,
                Err(TrySendError::Closed(_)) => ForwardResult::NotConnected,
            },
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
