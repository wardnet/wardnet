//! Background runner for the DNS filter subsystem.
//!
//! Subscribes to the event bus and rebuilds the affected slice of the
//! in-memory cache:
//! - [`WardnetEvent::DnsFilterChanged`] — profile content / membership /
//!   device assignment / default profile / global toggle.
//! - [`WardnetEvent::DnsFilterBlocklistUpdated`] — re-download finished.
//! - [`WardnetEvent::DeviceIpChanged`] — DHCP churn.
//!
//! Also runs the periodic blocklist cron check.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::{DnsFilterChange, WardnetEvent};

use crate::auth_context;
use crate::dns_filter::service::DnsFilterService;
use crate::event::EventPublisher;

pub struct DnsFilterRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DnsFilterRunner {
    pub fn start(
        service: Arc<dyn DnsFilterService>,
        events: &dyn EventPublisher,
        parent: &tracing::Span,
        cron_check_interval: Duration,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dns_filter_runner");
        let event_rx = events.subscribe();

        let handle = tokio::spawn(
            runner_loop(service, event_rx, cancel.clone(), cron_check_interval).instrument(span),
        );

        Self { cancel, handle }
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DNS filter runner shut down");
    }
}

async fn runner_loop(
    service: Arc<dyn DnsFilterService>,
    mut event_rx: broadcast::Receiver<WardnetEvent>,
    cancel: CancellationToken,
    cron_check_interval: Duration,
) {
    let admin_ctx = AuthContext::system();

    if let Err(e) = auth_context::with_context(admin_ctx.clone(), service.rebuild_all()).await {
        tracing::error!(error = %e, "failed to bootstrap DNS filter cache");
    }

    let mut cron_interval = tokio::time::interval(cron_check_interval);
    cron_interval.tick().await; // Skip the immediate-fire tick.

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DNS filter runner cancellation received");
                break;
            }
            _ = cron_interval.tick() => {
                check_blocklist_cron(service.as_ref(), &admin_ctx).await;
            }
            result = event_rx.recv() => {
                match result {
                    Ok(WardnetEvent::DnsFilterChanged { change, .. }) => {
                        handle_filter_change(service.as_ref(), &admin_ctx, change).await;
                    }
                    Ok(WardnetEvent::DnsFilterBlocklistUpdated { blocklist_id, .. }) => {
                        if let Err(e) = auth_context::with_context(
                            admin_ctx.clone(),
                            service.rebuild_blocklist_filter(blocklist_id),
                        ).await {
                            tracing::error!(error = %e, %blocklist_id, "rebuild blocklist filter failed");
                        }
                    }
                    Ok(WardnetEvent::DeviceIpChanged {
                        device_id, old_ip, new_ip, ..
                    }) => {
                        if let Err(e) = auth_context::with_context(
                            admin_ctx.clone(),
                            service.handle_device_ip_changed(device_id, &old_ip, &new_ip),
                        ).await {
                            tracing::error!(error = %e, %device_id, "handle DeviceIpChanged failed");
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "DNS filter runner lagged behind event bus");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("DNS filter runner: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_filter_change(
    service: &dyn DnsFilterService,
    admin_ctx: &AuthContext,
    change: DnsFilterChange,
) {
    let result = match change {
        DnsFilterChange::ProfileContent { profile_id }
        | DnsFilterChange::ProfileMembership { profile_id } => {
            auth_context::with_context(admin_ctx.clone(), service.rebuild_profile(profile_id)).await
        }
        DnsFilterChange::DeviceAssignment { device_id } => {
            auth_context::with_context(admin_ctx.clone(), service.rebuild_device(device_id)).await
        }
        DnsFilterChange::DefaultProfile | DnsFilterChange::GlobalToggle => {
            auth_context::with_context(admin_ctx.clone(), service.rebuild_default_context()).await
        }
    };
    if let Err(e) = result {
        tracing::error!(error = %e, "DNS filter rebuild after event failed");
    }
}

/// First backoff step after a single failed refresh. Doubles per consecutive
/// failure. Five minutes rides out a brief upstream blip without making a
/// transient failure cost a whole day of staleness.
const REFRESH_BACKOFF_BASE_MINS: i64 = 5;

/// Ceiling on the backoff. A feed that has been broken for a while is retried
/// every 6 hours — often enough that a fixed upstream recovers without
/// operator involvement, rare enough to stop hammering it.
const REFRESH_BACKOFF_MAX_MINS: i64 = 6 * 60;

/// Earliest a blocklist with failures behind it should be retried, or `None`
/// when it has no failures to back off from.
///
/// `5m → 10m → 20m → 40m → 80m → 160m → 320m → 6h (capped)`. Anchored on
/// `last_error_at` rather than "now" so the wait survives a daemon restart —
/// otherwise a restart loop would reset the backoff and restore the every-tick
/// hammering this exists to stop.
pub(crate) fn retry_not_before(bl: &wardnet_common::dns::Blocklist) -> Option<DateTime<Utc>> {
    let failures = bl.consecutive_failures;
    if failures == 0 {
        return None;
    }
    let last_error_at = bl.last_error_at?;
    // Cap the exponent before shifting. `checked_shl` only rejects an
    // out-of-range shift *amount*, not a shift that overflows into the sign
    // bit — `5 << 62` is negative, and clamping a negative would silently
    // produce the 5-minute floor instead of the 6-hour ceiling. 16 doublings
    // is already far past the cap.
    let steps = failures.saturating_sub(1).min(16);
    let mins = (REFRESH_BACKOFF_BASE_MINS << steps)
        .clamp(REFRESH_BACKOFF_BASE_MINS, REFRESH_BACKOFF_MAX_MINS);
    Some(last_error_at + chrono::Duration::minutes(mins))
}

async fn check_blocklist_cron(service: &dyn DnsFilterService, admin_ctx: &AuthContext) {
    let profiles =
        match auth_context::with_context(admin_ctx.clone(), service.list_profiles()).await {
            Ok(resp) => resp.profiles,
            Err(e) => {
                tracing::error!(error = %e, "failed to list profiles for cron check");
                return;
            }
        };

    let now = Utc::now();
    for profile in profiles {
        let blocklists = match auth_context::with_context(
            admin_ctx.clone(),
            service.list_blocklists(profile.id),
        )
        .await
        {
            Ok(resp) => resp.blocklists,
            Err(e) => {
                tracing::error!(profile_id = %profile.id, error = %e, "failed to list blocklists for cron check");
                continue;
            }
        };

        for bl in &blocklists {
            if !bl.enabled {
                continue;
            }
            let schedule = match crate::dns::cron_parse::parse_schedule(&bl.cron_schedule) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(blocklist_id = %bl.id, cron = %bl.cron_schedule, error = %e,
                        "invalid cron schedule, skipping");
                    continue;
                }
            };
            let is_due = match bl.last_updated {
                None => true,
                Some(last) => schedule
                    .after(&last)
                    .take_while(|t| *t <= now)
                    .next()
                    .is_some(),
            };
            if !is_due {
                continue;
            }
            // A failed refresh leaves `last_updated` untouched, so a broken
            // feed stays due on every tick from here on. Hold it off for a
            // stretch that grows with the failure count instead of
            // re-downloading it once a minute forever.
            if let Some(retry_at) = retry_not_before(bl)
                && now < retry_at
            {
                tracing::debug!(
                    blocklist_id = %bl.id,
                    name = %bl.name,
                    consecutive_failures = bl.consecutive_failures,
                    %retry_at,
                    "blocklist refresh backing off after failure"
                );
                continue;
            }
            tracing::info!(blocklist_id = %bl.id, name = %bl.name, "dispatching blocklist refresh");
            // The service owns the download/persist/event flow; it dispatches a
            // background job so the cron tick stays non-blocking.
            if let Err(e) = auth_context::with_context(
                admin_ctx.clone(),
                service.refresh_blocklist(profile.id, bl.id),
            )
            .await
            {
                tracing::error!(blocklist_id = %bl.id, error = %e, "failed to dispatch blocklist refresh");
            }
        }
    }
}
