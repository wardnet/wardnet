//! DHCP `.lan` auto-registration runner.
//!
//! Subscribes to the event bus and, on every DHCP lease assignment or
//! renewal that carries a client hostname, upserts a `{hostname}.lan` A
//! record so leased devices are immediately resolvable by name.
//!
//! Like the other background runners it holds no repository: it calls the
//! auth-gated [`DnsLocalService`] / [`DhcpService`] under an admin
//! [`auth_context`]. The service owns `DnsLocalChanged` emission, so a write
//! here automatically rebuilds [`crate::dns::AuthoritativeView`] via the
//! [`crate::dns::DnsRunner`].

use std::sync::Arc;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::api::UpsertRecordRequest;
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::{DnsRecordSource, DnsRecordType};
use wardnet_common::event::WardnetEvent;

use crate::auth_context;
use crate::dhcp::DhcpService;
use crate::dns_local::DnsLocalService;
use crate::error::AppError;
use crate::event::EventPublisher;

/// The seeded `.lan` zone id (see `20260414000000_dns.sql`). Records are
/// attached to it so they inherit the zone's enabled state.
const LAN_ZONE_ID: Uuid = uuid::uuid!("00000000-0000-0000-0000-000000000010");

/// Fallback record TTL (seconds) used when the DHCP config can't be read.
const FALLBACK_TTL_SECS: u32 = 300;

pub struct DhcpLanRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DhcpLanRunner {
    pub fn start(
        dns_local: Arc<dyn DnsLocalService>,
        dhcp: Arc<dyn DhcpService>,
        events: &dyn EventPublisher,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dhcp_lan_runner");
        let event_rx = events.subscribe();

        let handle =
            tokio::spawn(runner_loop(dns_local, dhcp, event_rx, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DHCP .lan runner shut down");
    }
}

async fn runner_loop(
    dns_local: Arc<dyn DnsLocalService>,
    dhcp: Arc<dyn DhcpService>,
    mut event_rx: broadcast::Receiver<WardnetEvent>,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DHCP .lan runner cancellation received");
                break;
            }
            result = event_rx.recv() => {
                match result {
                    Ok(WardnetEvent::DhcpLeaseAssigned { ip, hostname: Some(h), .. }
                    | WardnetEvent::DhcpLeaseRenewed { ip, hostname: Some(h), .. })
                        if !h.trim().is_empty() =>
                    {
                        register_lease(dns_local.as_ref(), dhcp.as_ref(), &admin_ctx, &h, &ip).await;
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "DHCP .lan runner lagged behind event bus");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("DHCP .lan runner: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}

/// Derive the `.lan` label from a DHCP option-12 hostname.
///
/// Option-12 may carry an FQDN (e.g. `mypc.home.arpa`); we keep only the first
/// label so appending `.lan` produces a clean single-label name. The result is
/// trimmed and lowercased. Returns `None` for empty / whitespace-only input.
pub(crate) fn lan_label(hostname: &str) -> Option<String> {
    let label = hostname.split('.').next()?.trim().to_lowercase();
    if label.is_empty() { None } else { Some(label) }
}

/// Upsert `{first-label-of-hostname}.lan → ip` as a DHCP-sourced A record.
/// Never fatal: every failure path logs a warning and returns.
async fn register_lease(
    dns_local: &dyn DnsLocalService,
    dhcp: &dyn DhcpService,
    admin_ctx: &AuthContext,
    hostname: &str,
    ip: &str,
) {
    let Some(label) = lan_label(hostname) else {
        return;
    };
    let domain = format!("{label}.lan");

    // TTL = half the lease duration so resolvers re-query around renewal time.
    // A zero (misconfigured) or unreadable lease duration falls back rather than
    // collapsing to a 1-second TTL.
    let ttl = match auth_context::with_context(admin_ctx.clone(), dhcp.get_dhcp_config()).await {
        Ok(config) if config.lease_duration_secs > 0 => (config.lease_duration_secs / 2).max(1),
        Ok(_) => FALLBACK_TTL_SECS,
        Err(e) => {
            tracing::warn!(error = %e, %domain, "failed to read DHCP config; using fallback TTL");
            FALLBACK_TTL_SECS
        }
    };

    // Soft-skip only if the seeded `.lan` zone is genuinely *gone* — a transient
    // DB/auth error is not "missing", so let those fall through to the upsert
    // (which surfaces the real failure) rather than silently dropping the lease.
    match auth_context::with_context(admin_ctx.clone(), dns_local.get_zone(LAN_ZONE_ID)).await {
        Ok(_) => {}
        Err(AppError::NotFound(_)) => {
            tracing::warn!(%domain, ".lan zone not found; skipping DHCP record");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, %domain, "failed to verify .lan zone; attempting upsert anyway");
        }
    }

    let req = UpsertRecordRequest {
        zone_id: Some(LAN_ZONE_ID),
        domain: domain.clone(),
        record_type: DnsRecordType::A,
        value: ip.to_owned(),
        ttl,
        enabled: true,
        source: DnsRecordSource::Dhcp,
    };
    match auth_context::with_context(admin_ctx.clone(), dns_local.upsert_record(req)).await {
        Ok(_) => {
            tracing::info!(%domain, ip, ttl, "registered DHCP .lan record");
        }
        // The service refuses to overwrite a `manual`/`system` record with a
        // DHCP one — a deliberate, benign skip, not a failure.
        Err(AppError::Conflict(_)) => {
            tracing::debug!(%domain, ip, "skipped DHCP record; a non-DHCP record owns this name");
        }
        Err(e) => {
            tracing::warn!(error = %e, %domain, ip, "failed to register DHCP .lan record");
        }
    }
}

#[cfg(test)]
mod tests;
