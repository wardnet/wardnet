use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{RwLock, broadcast};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_types::auth::AuthContext;
use wardnet_types::event::WardnetEvent;

use crate::auth_context;
use crate::dns::blocklist_downloader::{self, BlocklistFetcher};
use crate::dns::filter::DnsFilter;
use crate::dns::server::DnsServer;
use crate::event::EventPublisher;
use crate::repository::DnsRepository;
use crate::service::DnsService;

/// Background runner for the DNS server.
///
/// Manages the lifecycle of the DNS server based on configuration:
/// - Starts the server if DNS is enabled on startup.
/// - Listens for configuration change events to reload or restart.
/// - Periodically checks blocklist cron schedules and downloads updates.
/// - Rebuilds the in-memory DNS filter on blocklist/filter changes.
pub struct DnsRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DnsRunner {
    /// Start the DNS runner as a background task.
    pub fn start(
        service: Arc<dyn DnsService>,
        server: Arc<dyn DnsServer>,
        dns_repo: Arc<dyn DnsRepository>,
        filter: Arc<RwLock<DnsFilter>>,
        fetcher: Arc<dyn BlocklistFetcher>,
        events: &dyn EventPublisher,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dns_runner");
        let event_rx = events.subscribe();

        let handle = tokio::spawn(
            runner_loop(
                service,
                server,
                dns_repo,
                filter,
                fetcher,
                event_rx,
                cancel.clone(),
            )
            .instrument(span),
        );

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DNS runner shut down");
    }
}

/// Rebuild the in-memory DNS filter from repository state.
async fn rebuild_filter(
    service: &dyn DnsService,
    filter: &RwLock<DnsFilter>,
    admin_ctx: &AuthContext,
) {
    match auth_context::with_context(admin_ctx.clone(), service.load_filter_inputs()).await {
        Ok(inputs) => {
            let new_filter = DnsFilter::build(inputs);
            let stats = new_filter.stats();
            *filter.write().await = new_filter;
            tracing::info!(
                blocked = stats.blocked_count,
                allowed = stats.allowed_count,
                complex = stats.complex_count,
                "DNS filter rebuilt"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to load filter inputs for rebuild");
        }
    }
}

/// Main runner loop: start server if enabled, check blocklist cron schedules,
/// and listen for events.
async fn runner_loop(
    service: Arc<dyn DnsService>,
    server: Arc<dyn DnsServer>,
    dns_repo: Arc<dyn DnsRepository>,
    filter: Arc<RwLock<DnsFilter>>,
    fetcher: Arc<dyn BlocklistFetcher>,
    mut event_rx: broadcast::Receiver<WardnetEvent>,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

    // Check if DNS is enabled and start the server.
    match auth_context::with_context(admin_ctx.clone(), service.get_dns_config()).await {
        Ok(config) if config.enabled => {
            tracing::info!("DNS is enabled, starting server");
            server.update_config(config).await;
            if let Err(e) = server.start().await {
                tracing::error!(error = %e, "failed to start DNS server");
            }
        }
        Ok(_) => {
            tracing::info!("DNS is disabled, server not started");
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to load DNS config on startup");
        }
    }

    // Build initial filter.
    rebuild_filter(service.as_ref(), &filter, &admin_ctx).await;

    // Tick interval for checking blocklist cron schedules.
    let mut cron_interval = tokio::time::interval(std::time::Duration::from_secs(60));
    // Don't fire immediately — the first tick is the startup tick.
    cron_interval.tick().await;

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DNS runner cancellation received");
                break;
            }
            _ = cron_interval.tick() => {
                check_blocklist_cron(
                    &dns_repo,
                    &fetcher,
                    service.as_ref(),
                    &filter,
                    &admin_ctx,
                ).await;
            }
            result = event_rx.recv() => {
                match result {
                    Ok(WardnetEvent::DnsConfigChanged { .. }) => {
                        tracing::info!("DNS config changed, reloading");
                        match auth_context::with_context(admin_ctx.clone(), service.get_dns_config()).await {
                            Ok(config) => {
                                let should_run = config.enabled;
                                server.update_config(config).await;

                                if should_run && !server.is_running() {
                                    if let Err(e) = server.start().await {
                                        tracing::error!(error = %e, "failed to start DNS server after config change");
                                    }
                                } else if !should_run
                                    && server.is_running()
                                    && let Err(e) = server.stop().await
                                {
                                    tracing::error!(error = %e, "failed to stop DNS server after config change");
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "failed to reload DNS config");
                            }
                        }
                    }
                    Ok(WardnetEvent::DnsFiltersChanged { .. } | WardnetEvent::DnsBlocklistUpdated { .. }) => {
                        tracing::info!("DNS filters changed, rebuilding filter");
                        rebuild_filter(service.as_ref(), &filter, &admin_ctx).await;
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "DNS runner lagged behind event bus");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("DNS runner: event bus closed");
                        break;
                    }
                }
            }
        }
    }

    // Always stop the server when the runner loop exits.
    if let Err(e) = server.stop().await {
        tracing::error!(error = %e, "failed to stop DNS server during shutdown");
    }
}

/// Check each enabled blocklist's cron schedule and download updates when due.
async fn check_blocklist_cron(
    dns_repo: &Arc<dyn DnsRepository>,
    fetcher: &Arc<dyn BlocklistFetcher>,
    service: &dyn DnsService,
    filter: &Arc<RwLock<DnsFilter>>,
    admin_ctx: &AuthContext,
) {
    let blocklists = match dns_repo.list_blocklists().await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "failed to list blocklists for cron check");
            return;
        }
    };

    let now = Utc::now();
    let mut any_updated = false;

    for bl in &blocklists {
        if !bl.enabled {
            continue;
        }

        let schedule = match cron::Schedule::from_str(&bl.cron_schedule) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    blocklist_id = %bl.id,
                    cron = %bl.cron_schedule,
                    error = %e,
                    "invalid cron schedule, skipping"
                );
                continue;
            }
        };

        // Determine if the schedule has triggered since last_updated.
        let is_due = match bl.last_updated {
            None => true,
            Some(last) => {
                // Check if any scheduled time falls between last_updated and now.
                schedule
                    .after(&last)
                    .take_while(|t| *t <= now)
                    .next()
                    .is_some()
            }
        };

        if !is_due {
            continue;
        }

        tracing::info!(blocklist_id = %bl.id, name = %bl.name, "downloading blocklist");

        match fetcher.fetch(&bl.url).await {
            Ok(body) => {
                let domains = blocklist_downloader::parse_blocklist_body(&body);
                let count = domains.len() as u64;

                match dns_repo.replace_blocklist_domains(bl.id, &domains).await {
                    Ok(_) => {
                        if let Err(e) = dns_repo.set_blocklist_error(bl.id, None).await {
                            tracing::error!(
                                blocklist_id = %bl.id,
                                error = %e,
                                "failed to clear blocklist error"
                            );
                        }
                        tracing::info!(
                            blocklist_id = %bl.id,
                            name = %bl.name,
                            domains = count,
                            "blocklist updated"
                        );
                        any_updated = true;
                    }
                    Err(e) => {
                        let msg = format!("failed to store domains: {e}");
                        tracing::error!(blocklist_id = %bl.id, error = %e, "failed to store blocklist domains");
                        let _ = dns_repo.set_blocklist_error(bl.id, Some(&msg)).await;
                    }
                }
            }
            Err(e) => {
                let msg = format!("download failed: {e}");
                tracing::error!(blocklist_id = %bl.id, error = %e, "failed to download blocklist");
                let _ = dns_repo.set_blocklist_error(bl.id, Some(&msg)).await;
            }
        }
    }

    if any_updated {
        rebuild_filter(service, filter, admin_ctx).await;
    }
}
