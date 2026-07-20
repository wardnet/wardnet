//! Background runner that routes per-device DNS events into `dns_events`.
//!
//! Lifecycle:
//! - On startup: loads the set of device IDs with capture enabled from DB.
//! - Insert loop: drains `capture_rx`; for each row whose `device_id` is in
//!   the enabled set, inserts a row into `dns_events`.
//! - Event loop: subscribes to [`WardnetEvent::DeviceCaptureSettingsChanged`]
//!   and updates the in-memory enabled set accordingly.
//! - Prune loop (hourly): enforces per-device count + age caps for enabled
//!   devices, and deletes all data for devices that have capture disabled
//!   but still have stored rows.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;
use wardnetd_data::repository::{DnsEventsRepository, QueryLogRow};

use crate::auth_context;
use crate::device::DeviceService;
use crate::event::EventPublisher;

/// Prune loop tick interval.
pub const PRUNE_INTERVAL: Duration = Duration::from_hours(1);

/// Background runner capturing per-device DNS events.
pub struct DnsCaptureRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DnsCaptureRunner {
    pub fn start(
        capture_rx: mpsc::Receiver<QueryLogRow>,
        device_service: Arc<dyn DeviceService>,
        dns_events_repo: Arc<dyn DnsEventsRepository>,
        events: Arc<dyn EventPublisher>,
        parent: &tracing::Span,
    ) -> Self {
        Self::start_with_prune_interval(
            capture_rx,
            device_service,
            dns_events_repo,
            events,
            PRUNE_INTERVAL,
            parent,
        )
    }

    pub fn start_with_prune_interval(
        capture_rx: mpsc::Receiver<QueryLogRow>,
        device_service: Arc<dyn DeviceService>,
        dns_events_repo: Arc<dyn DnsEventsRepository>,
        events: Arc<dyn EventPublisher>,
        prune_interval: Duration,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dns_capture_runner");

        let handle = tokio::spawn(
            runner_loop(
                capture_rx,
                device_service,
                dns_events_repo,
                events,
                prune_interval,
                cancel.clone(),
            )
            .instrument(span),
        );

        Self { cancel, handle }
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DNS capture runner shut down");
    }
}

/// Load the capture-enabled device-id set from the DB under the runner's admin
/// context. Returns `None` (and logs) on error so the caller can decide whether
/// to start empty or keep its existing cache. `phase` names the call site so a
/// startup load failure stays distinguishable from a post-lag re-sync failure
/// in the logs.
async fn load_enabled_ids(
    device_service: &dyn DeviceService,
    admin_ctx: &AuthContext,
    phase: &str,
) -> Option<HashSet<String>> {
    match auth_context::with_context(
        admin_ctx.clone(),
        device_service.list_capture_enabled_device_ids(),
    )
    .await
    {
        Ok(ids) => Some(ids.into_iter().collect()),
        Err(e) => {
            tracing::error!(error = %e, "failed to load capture-enabled device IDs {phase}: {e}");
            None
        }
    }
}

async fn runner_loop(
    mut capture_rx: mpsc::Receiver<QueryLogRow>,
    device_service: Arc<dyn DeviceService>,
    dns_events_repo: Arc<dyn DnsEventsRepository>,
    events: Arc<dyn EventPublisher>,
    prune_interval: Duration,
    cancel: CancellationToken,
) {
    // This runner lives outside the HTTP middleware, so it establishes its own
    // system/admin context (`Uuid::nil()`) around every service call, per
    // `.agents/auth.md`.
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

    // Populate the hot-path cache from DB on startup.
    let mut enabled = load_enabled_ids(device_service.as_ref(), &admin_ctx, "on startup")
        .await
        .unwrap_or_default();
    tracing::info!(
        count = enabled.len(),
        "DNS capture runner started: count={count}",
        count = enabled.len(),
    );

    let mut event_rx = events.subscribe();

    let mut prune_ticker = tokio::time::interval(prune_interval);
    prune_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    prune_ticker.tick().await; // skip the immediate first tick

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DNS capture runner cancellation received");
                break;
            }
            recv = capture_rx.recv() => {
                let Some(row) = recv else {
                    tracing::info!("capture channel closed; exiting DNS capture runner");
                    break;
                };
                if let Some(ref device_id) = row.device_id
                    && enabled.contains(device_id.as_str())
                {
                    match dns_events_repo
                        .insert(device_id, &row.domain, &row.result, &row.timestamp)
                        .await
                    {
                        Ok(row_id) => {
                            let uuid = Uuid::parse_str(device_id).unwrap_or(Uuid::nil());
                            events.publish(WardnetEvent::DnsEventInserted {
                                device_id: uuid,
                                row_id,
                                domain: row.domain,
                                status: row.result,
                                captured_at: row.timestamp,
                                timestamp: Utc::now(),
                            });
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                device_id = %device_id,
                                "failed to insert DNS capture event: {e}"
                            );
                        }
                    }
                }
            }
            event = event_rx.recv() => {
                match event {
                    Ok(WardnetEvent::DeviceCaptureSettingsChanged { device_id, enabled: now_enabled, .. }) => {
                        let id = device_id.to_string();
                        if now_enabled {
                            enabled.insert(id);
                        } else {
                            enabled.remove(&id);
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            n,
                            "DNS capture runner event bus lagged by {n} messages; \
                             re-syncing enabled-device cache from DB"
                        );
                        // Events in the skipped window may include
                        // DeviceCaptureSettingsChanged — reload from DB so the
                        // in-memory cache is not permanently stale.
                        if let Some(ids) =
                            load_enabled_ids(device_service.as_ref(), &admin_ctx, "after event-bus lag")
                                .await
                        {
                            enabled = ids;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("event bus closed; exiting DNS capture runner");
                        break;
                    }
                }
            }
            _ = prune_ticker.tick() => {
                run_prune(device_service.as_ref(), dns_events_repo.as_ref(), &admin_ctx).await;
            }
        }
    }
}

async fn run_prune(
    device_service: &dyn DeviceService,
    dns_events_repo: &dyn DnsEventsRepository,
    admin_ctx: &AuthContext,
) {
    // Fetch all devices that have stored events.
    let ids_with_data = match dns_events_repo.find_device_ids_with_data().await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!(error = %e, "prune: failed to fetch device IDs with data: {e}");
            return;
        }
    };

    for device_id in &ids_with_data {
        match auth_context::with_context(
            admin_ctx.clone(),
            device_service.get_device_capture_settings(device_id),
        )
        .await
        {
            Ok(Some((true, cap_count, cap_days))) => {
                if let Err(e) = dns_events_repo
                    .prune_for_device(device_id, cap_count, cap_days)
                    .await
                {
                    tracing::warn!(
                        error = %e,
                        device_id = %device_id,
                        "prune failed for device: {e}"
                    );
                }
            }
            Ok(_) => {
                // Disabled or deleted device — purge all captured data.
                if let Err(e) = dns_events_repo.delete_all_for_device(device_id).await {
                    tracing::warn!(
                        error = %e,
                        device_id = %device_id,
                        "failed to delete all events for disabled device: {e}"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    device_id = %device_id,
                    "prune: failed to look up device: {e}"
                );
            }
        }
    }
}
