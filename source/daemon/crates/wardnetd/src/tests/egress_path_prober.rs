//! The two-stage probe against real sockets.
//!
//! A bare TCP connect is three small packets, so it passes on a path where
//! full-size segments do not survive. These cases pin the pair that tells those
//! apart: a path that never connects, and one that connects and then cannot
//! carry a transfer.

use wardnetd_services::egress_path::{EgressPathProber, ProbeTarget};

use crate::egress_path_prober::RealEgressPathProber;

/// Bind a loopback listener and return it with its port. Holding the listener
/// keeps the port open for the lifetime of the test.
fn listening() -> (std::net::TcpListener, u16) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// A port nothing is listening on: bound to learn the number, then dropped.
fn closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn prober(port: u16, transfer_url: String) -> RealEgressPathProber {
    RealEgressPathProber::new(
        ProbeTarget {
            host: "127.0.0.1".to_owned(),
            port,
        },
        transfer_url,
    )
}

/// A path that cannot complete a handshake has no transfer stage to judge,
/// which is what keeps one fault from raising two anomalies.
#[tokio::test]
async fn a_closed_port_fails_the_connect_stage_and_runs_no_transfer() {
    let port = closed_port();
    let outcome = prober(port, "https://127.0.0.1/unused".to_owned())
        .probe(None)
        .await;

    assert!(outcome.connect_failed());
    assert!(outcome.transfer.is_none());
    assert!(!outcome.degraded());
    assert!(outcome.connect.latency_ms.is_none());
    assert!(outcome.connect.error.is_some());
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    #[tokio::test]
    async fn an_open_port_completes_the_connect_stage() {
        let (_listener, port) = listening();
        // The transfer target is deliberately closed, so the round finishes
        // quickly instead of waiting out the transfer budget.
        let outcome = prober(port, format!("https://127.0.0.1:{}/x", closed_port()))
            .probe(None)
            .await;

        assert!(!outcome.connect_failed());
        assert!(outcome.connect.ok);
        assert!(outcome.connect.latency_ms.is_some());
    }

    /// The signature the whole design exists for: the handshake completes, so a
    /// connect-only probe would call the path healthy, and the transfer does
    /// not.
    #[tokio::test]
    async fn a_connect_that_cannot_transfer_reads_as_degraded() {
        let (_listener, port) = listening();
        let outcome = prober(port, format!("https://127.0.0.1:{}/x", closed_port()))
            .probe(None)
            .await;

        assert!(outcome.degraded());
        assert!(!outcome.connect_failed());
        let transfer = outcome
            .transfer
            .expect("a completed connect runs a transfer");
        assert!(!transfer.ok);
        assert!(transfer.error.is_some());
    }

    /// The probe target is an IP literal by design: resolving a name would make
    /// a DNS failure indistinguishable from a connect failure, which is the
    /// distinction this subsystem exists to draw.
    #[tokio::test]
    async fn a_hostname_target_fails_rather_than_resolving() {
        let prober = RealEgressPathProber::new(
            ProbeTarget {
                host: "example.com".to_owned(),
                port: 443,
            },
            "https://127.0.0.1/unused".to_owned(),
        );

        let outcome = prober.probe(None).await;

        assert!(outcome.connect_failed());
        assert!(
            outcome
                .connect
                .error
                .as_deref()
                .is_some_and(|e| e.contains("IP literal")),
            "the failure names the reason rather than looking like a dead path"
        );
    }

    /// Binding to an interface that does not exist must fail the probe rather
    /// than silently falling back to the default route — a tunnel's result has
    /// to describe that tunnel.
    #[tokio::test]
    async fn an_unknown_interface_fails_rather_than_probing_unbound() {
        let (_listener, port) = listening();
        let outcome = prober(port, "https://127.0.0.1/unused".to_owned())
            .probe(Some("wg_does_not_exist"))
            .await;

        assert!(outcome.connect_failed());
    }
}
