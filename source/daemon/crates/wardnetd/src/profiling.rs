use pyroscope::backend::{BackendConfig, PprofConfig, pprof_backend};
use pyroscope::pyroscope::{PyroscopeAgentBuilder, PyroscopeAgentRunning};

use crate::config::PyroscopeConfig;

/// Wrapper around the Pyroscope profiling agent for lifecycle management.
///
/// Holds the running agent and provides a `shutdown()` method that stops
/// profiling and flushes remaining data to the server.
pub struct ProfilingAgent {
    agent: pyroscope::PyroscopeAgent<PyroscopeAgentRunning>,
}

impl ProfilingAgent {
    /// Start the Pyroscope profiling agent if enabled in config.
    ///
    /// Returns `None` when profiling is disabled or the agent fails to start.
    pub fn start(config: &PyroscopeConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }

        let backend = pprof_backend(PprofConfig::default(), BackendConfig::default());

        let agent = match PyroscopeAgentBuilder::new(
            &config.endpoint,
            "wardnetd",
            100,
            "pprofrs",
            env!("WARDNET_VERSION"),
            backend,
        )
        .tags(vec![("version", env!("WARDNET_VERSION"))])
        .build()
        {
            Ok(ready) => match ready.start() {
                Ok(running) => running,
                Err(e) => {
                    tracing::error!(error = %e, "failed to start Pyroscope agent");
                    return None;
                }
            },
            Err(e) => {
                tracing::error!(error = %e, "failed to build Pyroscope agent");
                return None;
            }
        };

        tracing::info!(endpoint = %config.endpoint, "Pyroscope profiling agent started");
        Some(Self { agent })
    }

    /// Stop the profiling agent and flush remaining data.
    pub fn shutdown(self) {
        match self.agent.stop() {
            Ok(ready_agent) => ready_agent.shutdown(),
            Err(e) => tracing::error!(error = %e, "failed to stop Pyroscope agent"),
        }
        tracing::info!("Pyroscope profiling agent shut down");
    }
}
