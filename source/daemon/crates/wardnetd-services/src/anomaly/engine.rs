use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::anomaly::AnomalyType;
use wardnet_common::auth::AuthContext;

use crate::anomaly::service::AnomalyService;
use crate::auth_context;

/// How often open anomalies are re-checked when no interval is configured.
const DEFAULT_REEVALUATE_INTERVAL: Duration = Duration::from_mins(1);

/// Drives anomaly detection: the preventive half of the subsystem.
///
/// Each detector states its own cadence, so rather than one global tick the
/// engine keeps a deadline per detector in a min-heap and sleeps until the
/// nearest one. A five-minute sweep and an hourly one therefore cost exactly
/// as many wake-ups as they need, and adding a detector with a new cadence
/// requires no change here.
///
/// Alongside those, one recurring deadline re-checks every open anomaly so
/// conditions that have gone away get closed — including the reactive ones,
/// which have no sweep of their own.
///
/// Holds only an [`AnomalyService`]: the registry, the repository, and the
/// notification path all sit behind it, which keeps this a scheduler and
/// nothing more.
pub struct AnomaliesDetectionEngine {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

/// What the next deadline is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Due {
    /// Sweep one detector.
    Detect(AnomalyType),
    /// Re-check every open anomaly.
    Reevaluate,
}

impl AnomaliesDetectionEngine {
    /// Start the engine with the default reevaluation cadence.
    pub fn start(service: Arc<dyn AnomalyService>, parent: &tracing::Span) -> Self {
        Self::start_with_intervals(service, DEFAULT_REEVALUATE_INTERVAL, parent)
    }

    /// Start the engine, overriding how often open anomalies are re-checked.
    /// Detector sweep cadences always come from the detectors themselves.
    pub fn start_with_intervals(
        service: Arc<dyn AnomalyService>,
        reevaluate_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "anomalies_detection_engine");
        let handle =
            tokio::spawn(run(service, reevaluate_interval, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the engine and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("anomalies detection engine shut down");
    }
}

async fn run(
    service: Arc<dyn AnomalyService>,
    reevaluate_interval: Duration,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::system();
    let schedule = service.schedule();
    let now = Instant::now();

    // Seed every deadline at `now` so the first pass happens immediately:
    // a box that just booted should learn what is wrong with it without
    // waiting out a full interval first.
    let mut deadlines: BinaryHeap<Reverse<(Instant, Due)>> = BinaryHeap::new();
    for (anomaly_type, _) in &schedule {
        deadlines.push(Reverse((now, Due::Detect(*anomaly_type))));
    }
    deadlines.push(Reverse((now, Due::Reevaluate)));

    tracing::info!(
        detectors = schedule.len(),
        "anomalies detection engine started: detectors={detectors}",
        detectors = schedule.len(),
    );

    loop {
        // `schedule` is non-empty by construction (the reevaluate deadline is
        // always present), so the heap never empties.
        let Some(Reverse((deadline, due))) = deadlines.pop() else {
            return;
        };

        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("anomalies detection engine: cancellation received");
                return;
            }
            () = tokio::time::sleep_until(deadline) => {}
        }

        let interval = match due {
            Due::Detect(anomaly_type) => {
                if let Err(error) = auth_context::with_context(
                    admin_ctx.clone(),
                    service.run_detector(anomaly_type),
                )
                .await
                {
                    tracing::warn!(
                        %error,
                        anomaly_type = %anomaly_type.as_str(),
                        "anomaly engine: detector run failed: type={type}",
                        r#type = anomaly_type.as_str(),
                    );
                }
                schedule
                    .iter()
                    .find(|(t, _)| *t == anomaly_type)
                    .map_or(DEFAULT_REEVALUATE_INTERVAL, |(_, i)| *i)
            }
            Due::Reevaluate => {
                match auth_context::with_context(admin_ctx.clone(), service.reevaluate_all()).await
                {
                    Ok(summary) if summary.resolved > 0 => tracing::info!(
                        evaluated = summary.evaluated,
                        resolved = summary.resolved,
                        "anomaly engine: reevaluated {evaluated}, resolved {resolved}",
                        evaluated = summary.evaluated,
                        resolved = summary.resolved,
                    ),
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(%error, "anomaly engine: reevaluation pass failed");
                    }
                }
                reevaluate_interval
            }
        };

        // Schedule from *now* rather than from the missed deadline, so a slow
        // pass cannot build up a backlog of instantly-due wake-ups.
        deadlines.push(Reverse((Instant::now() + interval, due)));
    }
}

impl PartialOrd for Due {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Total order so `Due` can ride in the heap's tuple. The ordering itself is
/// arbitrary — it only ever breaks ties between deadlines at the same instant.
impl Ord for Due {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn key(due: Due) -> (u8, &'static str) {
            match due {
                Due::Detect(t) => (0, t.as_str()),
                Due::Reevaluate => (1, ""),
            }
        }
        key(*self).cmp(&key(*other))
    }
}
