//! Background runner that keeps the DDNS A record current.
//!
//! Ticks on a fixed interval (and once immediately at startup), calling
//! [`DdnsService::refresh_public_ip`] under an admin auth context. It holds only
//! `Arc<dyn DdnsService>` — never a repository or provider — per the runner
//! contract in `.agents/architecture.md`. When DDNS is unconfigured the service
//! short-circuits before any network call, so the runner is fully inert until a
//! provider is registered (the wizard wires that in a later commit).

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::auth_context;

use super::DdnsService;

/// How often the runner re-checks the WAN IP. A change is only published when
/// the discovered IP differs from the last one, so a tight cadence is cheap.
const REFRESH_INTERVAL: Duration = Duration::from_mins(5);

/// Background DDNS update runner. See [module docs](self).
pub struct DdnsUpdateRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DdnsUpdateRunner {
    /// Start the runner under a child span of `parent`.
    pub fn start(ddns: Arc<dyn DdnsService>, parent: &tracing::Span) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "ddns_update_runner");
        let handle = tokio::spawn(runner_loop(ddns, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the runner and wait for the task to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DDNS update runner shut down");
    }
}

async fn runner_loop(ddns: Arc<dyn DdnsService>, cancel: CancellationToken) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

    let mut interval = tokio::time::interval(REFRESH_INTERVAL);
    // The first `tick()` resolves immediately → refresh once at startup. If a
    // refresh runs long, `Delay` spaces the next tick a full interval after the
    // work finishes rather than bursting to catch up on missed ticks.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DDNS update runner cancellation received");
                break;
            }
            _ = interval.tick() => {
                refresh(ddns.as_ref(), &admin_ctx).await;
            }
        }
    }
}

/// Run one refresh under the admin context, logging the outcome. Never fatal.
async fn refresh(ddns: &dyn DdnsService, admin_ctx: &AuthContext) {
    match auth_context::with_context(admin_ctx.clone(), ddns.refresh_public_ip()).await {
        Ok(Some(ip)) => tracing::info!(%ip, "DDNS A record updated"),
        Ok(None) => tracing::debug!("DDNS refresh: nothing to do"),
        Err(error) => tracing::warn!(%error, "DDNS refresh failed"),
    }
}
