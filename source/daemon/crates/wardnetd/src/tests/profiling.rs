use crate::config::PyroscopeConfig;
use crate::profiling::ProfilingAgent;

#[test]
fn start_returns_none_when_disabled() {
    let config = PyroscopeConfig {
        enabled: false,
        endpoint: "http://localhost:4040".to_owned(),
    };

    let agent = ProfilingAgent::start(&config);
    assert!(agent.is_none());
}

#[test]
fn start_returns_some_when_enabled_and_shuts_down_cleanly() {
    // The pyroscope agent is push-based and tolerates a non-existent server
    // (it silently drops data). Point it at a bogus endpoint, start, then
    // immediately shut down to exercise the full lifecycle.
    let config = PyroscopeConfig {
        enabled: true,
        endpoint: "http://127.0.0.1:19999".to_owned(),
    };

    let agent = ProfilingAgent::start(&config);
    assert!(
        agent.is_some(),
        "expected Some(ProfilingAgent) when enabled"
    );

    // Exercise the shutdown path (stop + shutdown on the ready agent).
    agent.unwrap().shutdown();
}
