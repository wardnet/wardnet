//! Background runner that keeps the DDNS A record current.
//!
//! Ticks on a fixed interval (and once immediately at startup), calling
//! [`DdnsService::refresh_public_ip`] under an admin auth context. It holds only
//! `Arc<dyn DdnsService>` (plus the shared [`Entitlement`] handle) — never a
//! repository or provider — per the runner contract in `.agents/architecture.md`.
//! When DDNS is unconfigured the service short-circuits before any network call,
//! so the runner is fully inert until a provider is registered.
//!
//! ## Suspended state
//!
//! While the box is [suspended](Entitlement::is_suspended) the runner does **not**
//! do the heavier publish work (it would `403` against the cloud anyway). Instead
//! each tick it calls the cheap [`DdnsService::probe_entitlement`], which forces a
//! token mint purely for its side effect on the shared flag — so the daemon
//! self-heals the moment the operator resubscribes, with no operator action.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::entitlement::Entitlement;

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
    pub fn start(
        ddns: Arc<dyn DdnsService>,
        entitlement: Arc<Entitlement>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "ddns_update_runner");
        let handle = tokio::spawn(runner_loop(ddns, entitlement, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the runner and wait for the task to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DDNS update runner shut down");
    }
}

async fn runner_loop(
    ddns: Arc<dyn DdnsService>,
    entitlement: Arc<Entitlement>,
    cancel: CancellationToken,
) {
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
                // While suspended, skip the heavier publish (it would `403`
                // anyway) and instead cheaply re-probe entitlement so the box
                // self-heals when the subscription comes back.
                if entitlement.is_suspended() {
                    probe(ddns.as_ref(), &admin_ctx).await;
                } else {
                    refresh(ddns.as_ref(), &admin_ctx).await;
                }
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

/// Run one entitlement re-probe under the admin context. Never fatal — a
/// transport failure just leaves the box suspended until the next tick; a
/// successful mint restores it inside the cloud client.
async fn probe(ddns: &dyn DdnsService, admin_ctx: &AuthContext) {
    match auth_context::with_context(admin_ctx.clone(), ddns.probe_entitlement()).await {
        Ok(()) => tracing::debug!("entitlement re-probe complete"),
        Err(error) => tracing::debug!(%error, "entitlement re-probe failed; still suspended"),
    }
}
