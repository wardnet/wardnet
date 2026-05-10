//! mDNS advertiser — publishes `<hostname>.local.` so users on the LAN
//! can reach the setup wizard without knowing the Pi's IP.
//!
//! The daemon registers a single `_http._tcp.local.` service record on
//! the configured LAN interface. The same registration publishes an A
//! record for the chosen hostname, which is what makes
//! `http://wardnet.local:7411` resolvable from a laptop on the same LAN.
//!
//! On hostname collision, `mdns-sd` follows RFC 6762 §9 and auto-renames
//! the host to `<hostname>-2.local.`, `<hostname>-3.local.`, … — the same
//! suffix scheme spelled out in [`next_candidate`]. The runner subscribes
//! to the daemon's event channel and logs the resulting name so operators
//! can find the actual `.local` URL in the daemon log if a collision did
//! happen.
//!
//! The advertiser is a convenience, not load-bearing: if startup fails,
//! the rest of the daemon must continue. The wizard remains reachable by
//! IP. Callers should log [`Self::start`] errors and move on.

use std::net::Ipv4Addr;

use mdns_sd::{DaemonEvent, IfKind, ServiceDaemon, ServiceInfo};
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use wardnet_common::config::MdnsConfig;

/// Service type advertised. `_http._tcp.local.` is what browsers and
/// other Bonjour-aware clients look for when discovering web UIs.
const SERVICE_TYPE: &str = "_http._tcp.local.";

/// Default hostname when [`MdnsConfig::hostname`] is `None`.
pub const DEFAULT_HOSTNAME: &str = "wardnet";

/// Human-readable instance name shown in service browsers (e.g. macOS
/// Finder's Bonjour pane). The hostname is what resolves to an IP.
const INSTANCE_NAME: &str = "Wardnet";

/// Background runner that owns the [`ServiceDaemon`] for the lifetime
/// of the daemon and tears it down on shutdown.
///
/// Mirrors [`crate::heartbeat::HeartbeatRunner`] and
/// [`crate::route_monitor::RouteMonitor`]: a self-contained runner with
/// `start`/`shutdown` methods, no service-layer trait. The mock binary
/// does not start it — mDNS is a Linux-production concern.
pub struct MdnsAdvertiser {
    daemon: ServiceDaemon,
    fullname: String,
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl MdnsAdvertiser {
    /// Start the advertiser.
    ///
    /// On success, an `_http._tcp.local.` record is registered against
    /// `<hostname>.local.` on the given LAN interface, and a background
    /// task is spawned to log conflict-rename events.
    ///
    /// Returns `Err` if the daemon thread cannot be created or the
    /// initial registration fails. Callers should log and continue —
    /// the wizard remains reachable by IP.
    pub fn start(
        config: &MdnsConfig,
        lan_interface: &str,
        lan_ip: Ipv4Addr,
        http_port: u16,
        version: &str,
        parent: &tracing::Span,
    ) -> anyhow::Result<Self> {
        let span = tracing::info_span!(parent: parent, "mdns");
        let _enter = span.enter();

        let hostname = config
            .hostname
            .as_deref()
            .unwrap_or(DEFAULT_HOSTNAME)
            .to_owned();
        let host_dotlocal = format!("{hostname}.local.");

        let daemon = ServiceDaemon::new()
            .map_err(|e| anyhow::anyhow!("failed to create mDNS service daemon: {e}"))?;

        // Restrict broadcasts to the LAN interface. `wg*` tunnels and
        // loopback would otherwise leak the advertisement to peers and
        // local listeners. `disable_interface(All)` first so only the
        // explicit LAN interface is enabled.
        if let Err(e) = daemon.disable_interface(IfKind::All) {
            tracing::warn!(error = %e, "mDNS: failed to disable default interfaces; continuing");
        }
        daemon
            .enable_interface(IfKind::Name(lan_interface.to_owned()))
            .map_err(|e| {
                anyhow::anyhow!("failed to enable mDNS on interface {lan_interface}: {e}")
            })?;

        let properties = [("version", version)];
        // Empty IP set + `enable_addr_auto` lets `mdns-sd` track the LAN
        // interface's address and re-announce on `IpAdd`/`IpDel`. Since
        // the daemon also captures `lan_ip` for boot logging, we register
        // it explicitly *and* turn on auto-tracking so a later DHCP
        // re-lease still resolves correctly.
        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            INSTANCE_NAME,
            &host_dotlocal,
            std::net::IpAddr::V4(lan_ip),
            http_port,
            &properties[..],
        )
        .map_err(|e| anyhow::anyhow!("failed to build mDNS ServiceInfo: {e}"))?
        .enable_addr_auto();

        let fullname = service_info.get_fullname().to_owned();

        daemon
            .register(service_info)
            .map_err(|e| anyhow::anyhow!("failed to register mDNS service: {e}"))?;

        tracing::info!(
            host = %host_dotlocal,
            interface = %lan_interface,
            ip = %lan_ip,
            port = http_port,
            "mDNS advertiser started: host={host_dotlocal}, interface={lan_interface}, ip={lan_ip}, port={http_port}",
        );

        let cancel = CancellationToken::new();
        let monitor_span = tracing::Span::current();
        let cancel_clone = cancel.clone();
        let monitor_rx = daemon.monitor().ok();
        let handle = tokio::spawn(
            async move {
                let Some(rx) = monitor_rx else {
                    tracing::debug!("mDNS monitor unavailable; conflict events will not be logged");
                    cancel_clone.cancelled().await;
                    return;
                };
                loop {
                    tokio::select! {
                        () = cancel_clone.cancelled() => break,
                        event = rx.recv_async() => match event {
                            Ok(DaemonEvent::NameChange(change)) => {
                                tracing::warn!(
                                    original = %change.original,
                                    new_name = %change.new_name,
                                    interface = %change.intf_name,
                                    "mDNS hostname collision: resolver renamed {} to {} on {}",
                                    change.original,
                                    change.new_name,
                                    change.intf_name,
                                );
                            }
                            Ok(DaemonEvent::Error(e)) => {
                                tracing::warn!(error = %e, "mDNS daemon error: {e}");
                            }
                            Ok(_) => {}
                            Err(_) => break,
                        },
                    }
                }
            }
            .instrument(monitor_span),
        );

        Ok(Self {
            daemon,
            fullname,
            cancel,
            handle,
        })
    }

    /// Tear down the advertiser. Sends mDNS goodbye packets so peers
    /// drop their cached entry promptly instead of waiting out the TTL.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;

        match self.daemon.unregister(&self.fullname) {
            Ok(rx) => {
                let _ = rx.recv_async().await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "mDNS unregister failed: {e}");
            }
        }
        if let Err(e) = self.daemon.shutdown() {
            tracing::warn!(error = %e, "mDNS daemon shutdown failed: {e}");
        }
        tracing::info!("mDNS advertiser shut down");
    }
}

/// Returns the next hostname candidate when `current` collides with an
/// existing responder. Pure logic — kept as a documented spec of the
/// `<host>-N` suffix scheme that `mdns-sd`'s auto-rename produces under
/// RFC 6762 §9, and exercised by the unit tests in `tests/mdns_advertiser.rs`.
///
/// `next_candidate("wardnet") == "wardnet-2"`
/// `next_candidate("wardnet-2") == "wardnet-3"`
#[must_use]
pub fn next_candidate(current: &str) -> String {
    if let Some((prefix, suffix)) = current.rsplit_once('-')
        && let Ok(n) = suffix.parse::<u32>()
        && n >= 2
    {
        return format!("{prefix}-{}", n + 1);
    }
    format!("{current}-2")
}
