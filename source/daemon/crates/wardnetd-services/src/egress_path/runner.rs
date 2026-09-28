use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;
use wardnet_common::egress_path::{DIRECT_PATH, PathHealth, PathProbeOutcome, ProbeStage};
use wardnet_common::tunnel::{Tunnel, TunnelStatus};

use crate::auth_context;
use crate::egress_path::health::EgressPathHealth;
use crate::egress_path::prober::EgressPathProber;
use crate::stats::Meter;
use crate::tunnel::TunnelService;

/// How often every path is probed.
///
/// Matches `stats_intraday`'s one-minute bucket exactly, so each round
/// contributes one sample per bucket and a gauge needs no aggregation
/// convention. It is also the period of the app-retry pattern this was built to
/// explain — a slower cadence would alias against it.
pub const PROBE_INTERVAL: Duration = Duration::from_secs(60);

const METRIC_RESULT: &str = "path.probe.result";
const METRIC_LATENCY: &str = "path.probe.latency_ms";

/// Probes every egress path on a schedule and publishes the results.
///
/// Deliberately **not** a `HealthCheck`. See the module docs: a failing path
/// probe is an anomaly, never an input to the watchdog — otherwise a flaky VPN
/// restarts the daemon.
pub struct EgressPathProbeRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl EgressPathProbeRunner {
    pub fn start(
        prober: Arc<dyn EgressPathProber>,
        tunnels: Arc<dyn TunnelService>,
        health: Arc<EgressPathHealth>,
        meter: Arc<Meter>,
        parent: &tracing::Span,
    ) -> Self {
        Self::start_with_interval(prober, tunnels, health, meter, PROBE_INTERVAL, parent)
    }

    /// Start with a custom interval. Tests pass a short one.
    pub fn start_with_interval(
        prober: Arc<dyn EgressPathProber>,
        tunnels: Arc<dyn TunnelService>,
        health: Arc<EgressPathHealth>,
        meter: Arc<Meter>,
        interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "egress_path_probe_runner");
        let handle = tokio::spawn(
            runner_loop(prober, tunnels, health, meter, interval, cancel.clone()).instrument(span),
        );
        Self { cancel, handle }
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("egress path probe runner shut down");
    }
}

/// One path to probe: its stable subject id, a human label, and the interface
/// to bind to (`None` for direct).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbePath {
    pub(crate) path: String,
    pub(crate) label: String,
    pub(crate) interface: Option<String>,
}

/// Which paths to probe: every tunnel that is up, plus direct.
///
/// A tunnel that is not `Up` is skipped deliberately. It is already
/// `TunnelUnhealthy`'s subject, and alerting twice for one fault helps nobody —
/// which is what lets the path anomalies mean something narrower and more
/// useful: locally healthy by every measure we have, yet functionally broken.
///
/// Direct is always present and always first, so losing the tunnel list cannot
/// blind the one path that is never absent.
pub(crate) fn probe_paths(tunnels: &[Tunnel]) -> Vec<ProbePath> {
    let mut paths = vec![ProbePath {
        path: DIRECT_PATH.to_owned(),
        label: "Direct (WAN)".to_owned(),
        interface: None,
    }];
    paths.extend(
        tunnels
            .iter()
            .filter(|t| t.status == TunnelStatus::Up)
            .map(|t| ProbePath {
                path: t.id.to_string(),
                label: t.label.clone(),
                interface: Some(t.interface_name.clone()),
            }),
    );
    paths
}

async fn paths_to_probe(tunnels: &Arc<dyn TunnelService>) -> Vec<ProbePath> {
    match tunnels.list_tunnels().await {
        Ok(listing) => probe_paths(&listing.tunnels),
        Err(error) => {
            tracing::warn!(%error, "egress path prober: could not list tunnels");
            probe_paths(&[])
        }
    }
}

/// The stats labels one stage contributes, as `(result_labels, latency_labels)`.
///
/// Split out so the label shape is pinned by a test: `StatsBuffer` requires a
/// sorted JSON object, and a key out of order silently lands in a different
/// series rather than failing.
pub(crate) fn stage_labels(path: &str, stage: ProbeStage, ok: bool) -> (String, String) {
    let outcome = if ok { "ok" } else { "fail" };
    (
        format!(
            r#"{{"outcome":"{outcome}","path":"{path}","stage":"{}"}}"#,
            stage.as_str()
        ),
        format!(r#"{{"path":"{path}","stage":"{}"}}"#, stage.as_str()),
    )
}

fn record(meter: &Meter, path: &str, outcome: &PathProbeOutcome) {
    let mut stages = vec![(ProbeStage::Connect, &outcome.connect)];
    if let Some(transfer) = outcome.transfer.as_ref() {
        stages.push((ProbeStage::Transfer, transfer));
    }

    for (stage, result) in stages {
        let (result_labels, latency_labels) = stage_labels(path, stage, result.ok);
        meter.counter(METRIC_RESULT).add(&result_labels, 1.0);

        if let Some(ms) = result.latency_ms {
            #[expect(
                clippy::cast_precision_loss,
                reason = "probe latencies are milliseconds well inside f64's exact integer range"
            )]
            meter.gauge(METRIC_LATENCY).set(&latency_labels, ms as f64);
        }
    }
}

async fn probe_round(
    prober: &Arc<dyn EgressPathProber>,
    tunnels: &Arc<dyn TunnelService>,
    health: &Arc<EgressPathHealth>,
    meter: &Meter,
) {
    let targets = paths_to_probe(tunnels).await;
    probe_and_publish(prober, targets, health, meter).await;
}

/// Probe each target, record what came back, and publish the round.
///
/// Split from [`probe_round`] so the recording and publishing can be exercised
/// without standing up a whole `TunnelService`: which paths to probe is already
/// decided by [`probe_paths`], and this is what happens to them afterwards.
pub(crate) async fn probe_and_publish(
    prober: &Arc<dyn EgressPathProber>,
    targets: Vec<ProbePath>,
    health: &Arc<EgressPathHealth>,
    meter: &Meter,
) {
    let mut results = Vec::new();
    for target in targets {
        let outcome = prober.probe(target.interface.as_deref()).await;
        record(meter, &target.path, &outcome);

        if outcome.degraded() {
            let path = &target.path;
            tracing::warn!(
                %path,
                "egress path {path} connects but cannot complete a transfer — large \
                 segments are not getting through"
            );
        }

        results.push(PathHealth {
            path: target.path,
            label: target.label,
            outcome,
        });
    }
    health.publish(results);
}

async fn runner_loop(
    prober: Arc<dyn EgressPathProber>,
    tunnels: Arc<dyn TunnelService>,
    health: Arc<EgressPathHealth>,
    meter: Arc<Meter>,
    interval: Duration,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::system();
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = ticker.tick() => {
                auth_context::with_context(
                    admin_ctx.clone(),
                    probe_round(&prober, &tunnels, &health, &meter),
                )
                .await;
            }
        }
    }
}
