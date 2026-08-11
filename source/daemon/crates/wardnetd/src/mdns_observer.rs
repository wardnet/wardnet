//! mDNS observer — passively watches the LAN for service advertisements and
//! records them as device identification signals (issue #1115).
//!
//! mDNS is the signal that names most smart-home gear outright: `_googlecast._tcp`,
//! `_hap._tcp` and `_esphomelib._tcp` are unambiguous vendor tells. Observing
//! them costs nothing beyond a passive listener joined to the standard mDNS
//! multicast group — no unicast traffic is directed at the admin's own devices,
//! so this sits on the right side of ADR 0025's "no background probing"
//! invariant (decision 5), which is about active per-device probes.
//!
//! The observer browses **only** the service types the curated catalog can
//! attribute ([`vendor_catalog::mdns_service_types`]). That keeps the
//! observation surface bounded by the catalog — the same commitment
//! `probe_ports` makes for the admin-triggered probe — and asserted small by
//! test.
//!
//! ## IP → device
//!
//! A browse result yields an IP address, not a MAC. Resolution to a device
//! happens in [`DeviceIdentificationService::record_signal_for_ip`], which maps
//! the address through the devices table's `last_ip` and **skips ambiguous
//! mappings rather than guessing** (0 or >1 match). ADR 0025 records why: a
//! wrong attribution would put a confident vendor name on the wrong device,
//! which is exactly the failure mode issue #1099 was filed about.
//!
//! The observer is a background component, so it calls the *service*, never the
//! repository (`.agents/architecture.md`), under an explicit admin context (the
//! browse loop has no ambient one), mirroring the DHCP identification producer.
//!
//! Like the advertiser, the observer is a Linux-production concern: the mock
//! binary does not start it, and a startup failure is non-fatal — identification
//! simply loses one of several signals.

use std::sync::Arc;

use futures::StreamExt;
use futures::stream::select_all;
use mdns_sd::{IfKind, ResolvedService, ServiceDaemon, ServiceEvent};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use wardnet_common::auth::AuthContext;
use wardnet_common::device::DeviceSignalKind;
use wardnetd_data::vendor_catalog;
use wardnetd_services::auth_context;
use wardnetd_services::device::DeviceIdentificationService;

/// Background runner that owns the browsing [`ServiceDaemon`] for the lifetime
/// of the daemon and tears it down on shutdown.
///
/// Mirrors [`crate::mdns_advertiser::MdnsAdvertiser`]: a self-contained runner
/// with `start`/`shutdown` methods and no service-layer trait.
pub struct MdnsObserver {
    daemon: ServiceDaemon,
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl MdnsObserver {
    /// Start the observer.
    ///
    /// Joins the LAN interface's mDNS group, opens a browse per catalogued
    /// service type, and spawns a background task that records each resolved
    /// service against the device holding its address.
    ///
    /// Returns `Err` only if the daemon thread cannot be created or the LAN
    /// interface cannot be enabled. Individual browse failures are logged and
    /// skipped. Callers should log a start error and continue — identification
    /// keeps working from its other signals.
    pub fn start(
        identification: Arc<dyn DeviceIdentificationService>,
        lan_interface: &str,
        parent: &tracing::Span,
    ) -> anyhow::Result<Self> {
        let span = tracing::info_span!(parent: parent, "mdns_observer");
        let _enter = span.enter();

        let daemon = ServiceDaemon::new()
            .map_err(|e| anyhow::anyhow!("failed to create mDNS observer daemon: {e}"))?;

        // Confine browsing to the LAN interface. `wg_ward*` tunnels and loopback
        // must never carry these queries — the same restriction the advertiser
        // applies. `disable_interface(All)` first so only the explicit LAN
        // interface is enabled.
        if let Err(e) = daemon.disable_interface(IfKind::All) {
            tracing::warn!(error = %e, "mDNS observer: failed to disable default interfaces; continuing");
        }
        daemon
            .enable_interface(IfKind::Name(lan_interface.to_owned()))
            .map_err(|e| {
                anyhow::anyhow!("failed to enable mDNS observer on interface {lan_interface}: {e}")
            })?;

        // Browse exactly the service types the catalog can attribute. The
        // catalog stores them without the domain (`_govee._tcp`); the browse
        // query wants it fully qualified.
        let service_types = vendor_catalog::mdns_service_types();
        let mut streams = Vec::with_capacity(service_types.len());
        for ty in &service_types {
            let query = browse_query(ty);
            match daemon.browse(&query) {
                Ok(rx) => streams.push(rx.into_stream().boxed()),
                Err(e) => {
                    tracing::warn!(service_type = %query, error = %e, "mDNS observer: failed to browse {query}; continuing");
                }
            }
        }

        tracing::info!(
            interface = %lan_interface,
            browsed = streams.len(),
            "mDNS observer started: interface={lan_interface}, browsing {} service type(s)",
            streams.len(),
        );

        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let observer_span = tracing::Span::current();
        let handle = tokio::spawn(
            async move {
                // Merge every per-type browse stream into one. An empty set (all
                // browses failed) yields `None` immediately and drops straight to
                // waiting for shutdown — no busy loop, no panic.
                let mut events = select_all(streams);
                loop {
                    tokio::select! {
                        () = cancel_clone.cancelled() => break,
                        event = events.next() => match event {
                            Some(ServiceEvent::ServiceResolved(info)) => {
                                record_resolved(identification.as_ref(), info.as_ref()).await;
                            }
                            // SearchStarted / ServiceFound / ServiceRemoved /
                            // SearchStopped carry no address to attribute.
                            Some(_) => {}
                            None => {
                                // Every browse stream ended without a shutdown
                                // request — the daemon's channels closed under us
                                // (internal failure, all senders dropped). No more
                                // observations will arrive; surface it rather than
                                // parking indistinguishably from a healthy idle.
                                tracing::warn!(
                                    "mDNS observer: all browse streams ended unexpectedly; no further service advertisements will be observed"
                                );
                                cancel_clone.cancelled().await;
                                break;
                            }
                        },
                    }
                }
            }
            .instrument(observer_span),
        );

        Ok(Self {
            daemon,
            cancel,
            handle,
        })
    }

    /// Tear down the observer: stop the browse loop and shut the daemon down.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        if let Err(e) = self.daemon.shutdown() {
            tracing::warn!(error = %e, "mDNS observer daemon shutdown failed: {e}");
        }
        tracing::info!("mDNS observer shut down");
    }
}

/// The fully-qualified browse query for a catalogued service type.
///
/// The catalog stores service types without a domain (`_govee._tcp`), while an
/// mDNS browse query wants it terminated with the local domain
/// (`_govee._tcp.local.`).
pub(crate) fn browse_query(service_type: &str) -> String {
    format!("{service_type}.local.")
}

/// Record every address a resolved service advertised as an [`MdnsService`]
/// signal for the device holding that address.
///
/// A resolved service can carry several addresses (IPv4 plus IPv6); each is
/// resolved independently, and the service's ambiguity skip means an address no
/// single device claims is simply dropped. Runs under an explicit admin context
/// because the identification service is auth-gated and this browse loop is a
/// background task with no ambient context of its own.
///
/// [`MdnsService`]: DeviceSignalKind::MdnsService
async fn record_resolved(identification: &dyn DeviceIdentificationService, info: &ResolvedService) {
    for scoped in &info.addresses {
        let ip = scoped.to_ip_addr();
        let result = auth_context::with_context(
            // `system()` rather than a hand-rolled nil-UUID variant: it is the
            // reserved system actor, and going through the constructor keeps
            // background work distinguishable from a real person in audit logs.
            AuthContext::system(),
            identification.record_signal_for_ip(ip, DeviceSignalKind::MdnsService, &info.ty_domain),
        )
        .await;
        if let Err(error) = result {
            // The service returns Ok for the ambiguous/zero-match skips, so an
            // Err here is a genuine backend failure (e.g. a write error), not a
            // dropped attribution — warn so a sustained outage is visible.
            let service_type = &info.ty_domain;
            tracing::warn!(
                %ip,
                %service_type,
                %error,
                "mDNS observer: failed to record {service_type} signal for {ip}: {error}"
            );
        }
    }
}
