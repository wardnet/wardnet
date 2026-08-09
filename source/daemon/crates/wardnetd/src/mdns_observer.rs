//! mDNS observer — browses the LAN for the service types the vendor catalog
//! knows and records them as device-identification signals (issue #1115).
//!
//! `vendors.toml` already declares `mdns_services` per vendor and the
//! identification service already knows what to do with a
//! [`DeviceSignalKind::MdnsService`]; this is the component that supplies one.
//! A device that answers `_googlecast._tcp` or `_esphomelib._tcp` names itself
//! outright, which is the gap ADR 0025 was written about — most smart-home gear
//! has an OUI the IEEE database cannot resolve.
//!
//! **This does not violate ADR 0025's "identification never scans in the
//! background" invariant.** That rule forbids emitting unsolicited traffic *to
//! an admin's device*; DNS-SD browsing sends multicast queries to the mDNS
//! group address and listens for whatever answers. No device is contacted.
//! Active probing remains what it was: an explicit per-device admin action.
//!
//! Two things are deliberately narrow:
//!
//! * **Only catalogued service types are browsed.** The browse set is read from
//!   the catalog, so teaching Wardnet a new vendor stays a `vendors.toml` edit
//!   (ADR 0025, decision 3) and every signal recorded is one that can name
//!   something.
//! * **Only the service *type* is recorded**, never the instance name. Instance
//!   names are free-form and routinely personal (`Pedro's iPhone`); storing
//!   them is a separate decision nobody has made.
//!
//! Like the advertiser, this is a self-contained runner with `start`/`shutdown`
//! and no service-layer trait, and it is not load-bearing: if it fails to
//! start, the daemon carries on without it.

use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::sync::Arc;

use futures::StreamExt;
use futures::stream::SelectAll;
use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use wardnet_common::auth::AuthContext;
use wardnet_common::device::DeviceSignalKind;
use wardnetd_data::vendor_catalog;
use wardnetd_services::auth_context;
use wardnetd_services::device::DeviceIdentificationService;

/// How many `(address, service type)` pairs to remember before forgetting the
/// lot. mDNS re-announces on a timer, so without a memory every announcement
/// would be a repeat write; without a bound, a hostile responder cycling
/// service types would grow the set forever. A few hundred covers a LAN many
/// times larger than Wardnet targets, and overflowing merely costs a redundant
/// upsert on the next announcement.
pub(crate) const MAX_REMEMBERED_OBSERVATIONS: usize = 512;

/// Background runner that browses for catalogued mDNS service types and feeds
/// what it finds to the identification service.
///
/// Mirrors [`crate::mdns_advertiser::MdnsAdvertiser`]: a self-contained runner
/// with `start`/`shutdown`, owning its own [`ServiceDaemon`]. Deliberately not
/// sharing the advertiser's daemon — observing must work on a LAN where
/// advertising is turned off, and vice versa. The mock binary does not start
/// it; mDNS is a Linux-production concern.
pub struct MdnsObserver {
    daemon: ServiceDaemon,
    browsing: Vec<String>,
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl MdnsObserver {
    /// Start browsing.
    ///
    /// Returns `Err` if the mDNS daemon cannot be created, the LAN interface
    /// cannot be enabled, or no service type could be browsed at all. Callers
    /// should log and continue — identification simply keeps working from the
    /// signals it already has.
    pub fn start(
        lan_interface: &str,
        identification: Arc<dyn DeviceIdentificationService>,
        parent: &tracing::Span,
    ) -> anyhow::Result<Self> {
        let span = tracing::info_span!(parent: parent, "mdns_observer");
        let _enter = span.enter();

        let daemon = ServiceDaemon::new()
            .map_err(|e| anyhow::anyhow!("failed to create mDNS service daemon: {e}"))?;

        // Same interface pinning as the advertiser: browsing on `wg*` tunnels
        // would query peers across the tunnel and attribute their services to
        // LAN addresses.
        if let Err(e) = daemon.disable_interface(IfKind::All) {
            tracing::warn!(error = %e, "mDNS observer: failed to disable default interfaces; continuing");
        }
        daemon
            .enable_interface(IfKind::Name(lan_interface.to_owned()))
            .map_err(|e| {
                anyhow::anyhow!("failed to enable mDNS on interface {lan_interface}: {e}")
            })?;

        let mut browsing = Vec::new();
        let mut receivers = Vec::new();
        for service_type in vendor_catalog::mdns_service_types() {
            let name = browse_name(service_type);
            match daemon.browse(&name) {
                Ok(rx) => {
                    receivers.push(rx);
                    browsing.push(name);
                }
                Err(e) => {
                    tracing::warn!(service_type = %name, error = %e, "mDNS observer: browse failed for {name}: {e}");
                }
            }
        }

        if browsing.is_empty() {
            if let Err(e) = daemon.shutdown() {
                tracing::warn!(error = %e, "mDNS observer: daemon shutdown failed: {e}");
            }
            anyhow::bail!("no catalogued mDNS service type could be browsed");
        }

        tracing::info!(
            interface = %lan_interface,
            service_types = browsing.len(),
            "mDNS observer started: interface={lan_interface}, service_types={}",
            browsing.len(),
        );

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let loop_span = tracing::Span::current();
        let handle = tokio::spawn(
            async move {
                let mut events = SelectAll::new();
                for rx in receivers {
                    events.push(rx.into_stream());
                }
                let mut seen = ObservationCache::default();

                loop {
                    tokio::select! {
                        () = cancel_clone.cancelled() => break,
                        event = events.next() => match event {
                            // `ServiceEvent` is `#[non_exhaustive]`; everything
                            // other than a resolved instance (search started,
                            // service found-but-unresolved, removals) tells us
                            // nothing about who a device is.
                            Some(ServiceEvent::ServiceResolved(service)) => {
                                let service_type = bare_service_type(&service.ty_domain);
                                for ip in service.get_addresses_v4() {
                                    record(&identification, &mut seen, ip, &service_type).await;
                                }
                            }
                            Some(_) => {}
                            // Every browse ended — nothing further can arrive.
                            None => break,
                        },
                    }
                }
            }
            .instrument(loop_span),
        );

        Ok(Self {
            daemon,
            browsing,
            cancel,
            handle,
        })
    }

    /// Stop browsing and tear the daemon down.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;

        for service_type in &self.browsing {
            if let Err(e) = self.daemon.stop_browse(service_type) {
                tracing::warn!(service_type = %service_type, error = %e, "mDNS observer: stop_browse failed: {e}");
            }
        }
        if let Err(e) = self.daemon.shutdown() {
            tracing::warn!(error = %e, "mDNS observer: daemon shutdown failed: {e}");
        }
        tracing::info!("mDNS observer shut down");
    }
}

/// Record one observation, unless it repeats one already recorded.
///
/// Failures are logged and swallowed: identification is a side-effect of
/// watching the network, and it must never take the browse loop down.
async fn record(
    identification: &Arc<dyn DeviceIdentificationService>,
    seen: &mut ObservationCache,
    ip: Ipv4Addr,
    service_type: &str,
) {
    if seen.contains(ip, service_type) {
        return;
    }

    let ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    let result = auth_context::with_context(
        ctx,
        identification.record_signal_for_ip(
            &ip.to_string(),
            DeviceSignalKind::MdnsService,
            service_type,
        ),
    )
    .await;

    match result {
        // Remembered only on success, and deliberately so: an announcement
        // seen before its device was discovered — or while its address was
        // ambiguous — is dropped by the service, and must stay eligible for
        // the next announcement rather than being cached as handled.
        Ok(()) => seen.remember(ip, service_type),
        Err(e) => {
            tracing::warn!(
                %ip,
                service_type,
                error = %e,
                "mDNS observer: failed to record signal for {ip}: {e}"
            );
        }
    }
}

/// Fully-qualified name to browse for a bare catalog service type.
///
/// `mdns-sd` requires the `._tcp.local.` / `._udp.local.` form; the catalog
/// stores the bare type. Idempotent, so a catalog entry written either way
/// produces the same browse.
pub(crate) fn browse_name(service_type: &str) -> String {
    let bare = bare_service_type(service_type);
    format!("{bare}.local.")
}

/// Strip the `.local.` domain a browse result carries, leaving the bare service
/// type the catalog declares.
///
/// Applied before recording so a stored signal reads `_govee._tcp` — the same
/// string `vendors.toml` uses and the UI shows — rather than the wire form.
pub(crate) fn bare_service_type(ty_domain: &str) -> String {
    ty_domain
        .trim_end_matches('.')
        .trim_end_matches(".local")
        .to_lowercase()
}

/// Bounded memory of observations already recorded, so a device re-announcing
/// on its mDNS timer does not produce a write per announcement.
#[derive(Default)]
pub(crate) struct ObservationCache {
    seen: HashSet<(Ipv4Addr, String)>,
}

impl ObservationCache {
    pub(crate) fn contains(&self, ip: Ipv4Addr, service_type: &str) -> bool {
        self.seen.contains(&(ip, service_type.to_owned()))
    }

    pub(crate) fn remember(&mut self, ip: Ipv4Addr, service_type: &str) {
        // Clearing wholesale rather than evicting one entry: there is no
        // recency information worth preserving here, and the only cost of a
        // cold cache is one redundant upsert per device on its next
        // announcement.
        if self.seen.len() >= MAX_REMEMBERED_OBSERVATIONS {
            self.seen.clear();
        }
        self.seen.insert((ip, service_type.to_owned()));
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.seen.len()
    }
}
