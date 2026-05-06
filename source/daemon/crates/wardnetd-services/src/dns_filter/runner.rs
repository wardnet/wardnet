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

use chrono::Utc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::{DnsFilterChange, WardnetEvent};

use crate::auth_context;
use crate::dns_filter::blocklist_downloader::{self, BlocklistFetcher};
use crate::dns_filter::service::DnsFilterService;
use crate::event::EventPublisher;
use wardnetd_data::repository::DnsFilterRepository;

pub struct DnsFilterRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DnsFilterRunner {
    pub fn start(
        service: Arc<dyn DnsFilterService>,
        repo: Arc<dyn DnsFilterRepository>,
        fetcher: Arc<dyn BlocklistFetcher>,
        events: Arc<dyn EventPublisher>,
        parent: &tracing::Span,
        cron_check_interval: Duration,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dns_filter_runner");
        let event_rx = events.subscribe();

        let handle = tokio::spawn(
            runner_loop(
                service,
                repo,
                fetcher,
                events,
                event_rx,
                cancel.clone(),
                cron_check_interval,
            )
            .instrument(span),
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
    repo: Arc<dyn DnsFilterRepository>,
    fetcher: Arc<dyn BlocklistFetcher>,
    events: Arc<dyn EventPublisher>,
    mut event_rx: broadcast::Receiver<WardnetEvent>,
    cancel: CancellationToken,
    cron_check_interval: Duration,
) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

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
                check_blocklist_cron(repo.as_ref(), fetcher.as_ref(), events.as_ref()).await;
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

async fn check_blocklist_cron(
    repo: &dyn DnsFilterRepository,
    fetcher: &dyn BlocklistFetcher,
    events: &dyn EventPublisher,
) {
    let profiles = match repo.list_profiles().await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "failed to list profiles for cron check");
            return;
        }
    };

    let now = Utc::now();
    for profile in profiles {
        let blocklists = match repo.list_blocklists(profile.id).await {
            Ok(b) => b,
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
            tracing::info!(blocklist_id = %bl.id, name = %bl.name, "downloading blocklist");
            if let Err(e) =
                blocklist_downloader::refresh_blocklist(bl, repo, fetcher, events, None).await
            {
                tracing::error!(blocklist_id = %bl.id, error = %e, "failed to refresh blocklist");
            }
        }
    }
}
