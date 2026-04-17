use tokio::sync::broadcast;
use wardnet_common::event::WardnetEvent;

/// Abstraction over domain event publishing and subscribing.
///
/// Allows mocking in tests to verify events were published without
/// requiring a real broadcast channel.
pub trait EventPublisher: Send + Sync {
    /// Publish a domain event to all subscribers.
    fn publish(&self, event: WardnetEvent);

    /// Create a new subscriber that receives future events.
    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent>;
}

/// Default implementation backed by `tokio::broadcast`.
///
/// Clone-friendly — wraps `broadcast::Sender` which is `Clone`.
#[derive(Debug, Clone)]
pub struct BroadcastEventBus {
    sender: broadcast::Sender<WardnetEvent>,
}

impl BroadcastEventBus {
    /// Create a new event bus with the given channel capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }
}

impl EventPublisher for BroadcastEventBus {
    fn publish(&self, event: WardnetEvent) {
        // Ignore errors -- occurs when no subscribers exist.
        let _ = self.sender.send(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        self.sender.subscribe()
    }
}
