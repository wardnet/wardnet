use tracing_subscriber::Registry;

/// A boxed tracing layer compatible with the standard [`Registry`] subscriber.
pub type BoxedLayer = Box<dyn tracing_subscriber::Layer<Registry> + Send + Sync>;

/// A component that participates in the tracing subscriber pipeline.
///
/// Implementors provide a tracing layer that is composed into the subscriber
/// at startup. The `start()`/`stop()` methods control whether the component
/// is actively processing events.
///
/// # Lifecycle
///
/// 1. Component is created (e.g. `ErrorNotifierService::new(15)`)
/// 2. `tracing_layer()` is called once to extract the layer for subscriber composition
/// 3. The daemon builds the subscriber with all collected layers
/// 4. `start()` is called to activate the component
/// 5. `stop()` is called during shutdown
pub trait LogComponent: Send + Sync {
    /// Return the tracing layer for subscriber composition.
    ///
    /// Called once during startup, before the subscriber is built.
    fn tracing_layer(&self) -> BoxedLayer;

    /// Activate the component (begin processing events).
    fn start(&self);

    /// Deactivate the component (silently drop events).
    fn stop(&self);

    /// Whether the component is currently active.
    fn is_active(&self) -> bool;
}
