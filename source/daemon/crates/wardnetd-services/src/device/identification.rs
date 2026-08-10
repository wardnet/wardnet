//! Device identification — recording observed signals and resolving them to a
//! vendor through the curated catalog (issue #1099).
//!
//! Every identification signal, whoever observed it, lands here: the DHCP
//! server (options 12/55/60), the mDNS observer (advertised service types) and
//! the admin-triggered prober (answering ports). The service is the only thing
//! that talks to [`DeviceIdentificationRepository`], so background components follow
//! the "runners call services, not repositories" rule in `.agents/architecture.md`.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use wardnet_common::device::{DeviceSignal, DeviceSignalKind, ManufacturerSource};
use wardnet_common::net::is_private_ip;

use crate::auth_context;
use crate::device::prober::DeviceProber;
use crate::error::AppError;
use wardnetd_data::repository::{
    DeviceIdentificationRepository, DeviceRepository, DeviceSignalRow,
};
use wardnetd_data::vendor_catalog;

/// What a probe contacted and what answered.
///
/// `ports_probed` is carried alongside `answering_ports` so the caller can tell
/// "we contacted four ports and none answered" apart from "nothing happened".
/// Without it an empty result is indistinguishable from a broken button.
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    /// Every port the probe contacted — the catalog's full probe surface.
    pub ports_probed: Vec<u16>,
    /// The subset that answered, sorted. Each one was recorded as a
    /// [`DeviceSignalKind::ProbedPort`] signal.
    pub answering_ports: Vec<u16>,
}

/// Records identification signals and reads them back.
#[async_trait]
pub trait DeviceIdentificationService: Send + Sync {
    /// Record an observed signal for a device.
    ///
    /// If the signal resolves to a vendor through the curated catalog and the
    /// device has no authoritative (IEEE) manufacturer, the device's
    /// manufacturer is filled in and marked
    /// [`ManufacturerSource::Signal`]. An IEEE name is never overwritten — the
    /// registrant on record outranks anything we infer.
    async fn record_signal(
        &self,
        device_id: &str,
        kind: DeviceSignalKind,
        value: &str,
    ) -> Result<(), AppError>;

    /// Record a signal against whichever device currently owns `mac`.
    ///
    /// Silently does nothing when the MAC is unknown. DHCP routinely sees a
    /// client before ARP discovery has inserted its device row, and a lease
    /// must not fail because an observability side-effect had nowhere to land.
    async fn record_signal_for_mac(
        &self,
        mac: &str,
        kind: DeviceSignalKind,
        value: &str,
    ) -> Result<(), AppError>;

    /// Every signal recorded for a device, most recent first.
    async fn signals_for(&self, device_id: &str) -> Result<Vec<DeviceSignal>, AppError>;

    /// Probe a device's catalog ports and record whichever answered.
    ///
    /// The direct-admin-action end of ADR 0025 §5's invariant: this is the only
    /// thing that produces [`DeviceSignalKind::ProbedPort`] signals, and the
    /// only method in the subsystem that emits unsolicited traffic.
    ///
    /// Refuses with [`AppError::Conflict`] unless the device is still on the
    /// network. `last_ip` is last-observation-wins, so probing a departed
    /// device contacts whoever holds that address *now* — and a vendor resolved
    /// from that stranger's port would be written permanently onto this
    /// device's row by `set_manufacturer_if_absent`. The guard is about
    /// misattribution, not privacy.
    async fn probe_device(&self, device_id: &str) -> Result<ProbeOutcome, AppError>;

    /// Name already-discovered devices the curated catalog can identify.
    ///
    /// The migration can only *remove* placeholder manufacturers — the catalog
    /// lives in Rust, so SQL cannot apply it. Without this pass, an upgrade
    /// leaves the very device that motivated issue #1099 (a Govee lamp whose
    /// IEEE listing is `Private`) reading "Unknown manufacturer" forever,
    /// because only `insert_new_device` consults the catalog and the device was
    /// discovered long ago.
    ///
    /// Runs at startup and is idempotent: it only fills a `NULL` manufacturer,
    /// so it also picks up devices that a *later* release teaches the catalog
    /// about. Returns how many devices it named.
    async fn reconcile_from_catalog(&self) -> Result<usize, AppError>;
}

/// Default [`DeviceIdentificationService`] implementation.
pub struct DeviceIdentificationServiceImpl {
    identification: Arc<dyn DeviceIdentificationRepository>,
    devices: Arc<dyn DeviceRepository>,
    prober: Arc<dyn DeviceProber>,
    /// `detection.departure_timeout_secs` — how long a device may go unseen
    /// before the daemon considers it gone. Reused as the probe's freshness
    /// bound rather than inventing a second, subtly different threshold.
    departure_timeout: Duration,
}

impl DeviceIdentificationServiceImpl {
    #[must_use]
    pub fn new(
        identification: Arc<dyn DeviceIdentificationRepository>,
        devices: Arc<dyn DeviceRepository>,
        prober: Arc<dyn DeviceProber>,
        departure_timeout: Duration,
    ) -> Self {
        Self {
            identification,
            devices,
            prober,
            departure_timeout,
        }
    }
}

/// Longest signal value stored. Real option-60 strings and mDNS service types
/// are far shorter; anything longer is malformed or hostile.
const MAX_SIGNAL_VALUE_LEN: usize = 128;

/// How many distinct values to keep per device per signal kind. A device
/// legitimately advertises a handful of mDNS services; a client cycling its
/// vendor class every renewal is not something we want to record forever.
const MAX_SIGNALS_PER_KIND: usize = 16;

/// Clamp an observed value to [`MAX_SIGNAL_VALUE_LEN`], respecting UTF-8
/// boundaries so a truncated multi-byte character cannot panic the caller.
fn truncate_signal_value(value: &str) -> String {
    if value.len() <= MAX_SIGNAL_VALUE_LEN {
        return value.to_owned();
    }
    let mut end = MAX_SIGNAL_VALUE_LEN;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

#[async_trait]
impl DeviceIdentificationService for DeviceIdentificationServiceImpl {
    async fn record_signal(
        &self,
        device_id: &str,
        kind: DeviceSignalKind,
        value: &str,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;

        // Bound what an unauthenticated LAN client can write. DHCP options are
        // attacker-controlled: the value is truncated so one packet cannot
        // store an arbitrarily long string, and the per-device/kind row count
        // is capped so a client that varies its option-60 on every renewal
        // cannot grow the table without limit.
        let value = truncate_signal_value(value);
        let vendor = vendor_catalog::lookup_signal(kind, &value);

        self.identification
            .record(&DeviceSignalRow {
                device_id: device_id.to_owned(),
                kind,
                value: value.clone(),
                inferred: vendor.is_some(),
            })
            .await
            .map_err(AppError::Internal)?;

        self.identification
            .prune_signals(device_id, kind, MAX_SIGNALS_PER_KIND)
            .await
            .map_err(AppError::Internal)?;

        // Name the device only if it has none yet. Deliberately NOT "overwrite
        // anything weaker than IEEE" — see
        // [`DeviceIdentificationRepository::set_manufacturer_if_absent`], which
        // makes the first-writer-wins rule atomic rather than leaving it to a
        // read-then-write race between concurrent signal sources.
        if let Some(vendor) = vendor {
            self.identification
                .set_manufacturer_if_absent(device_id, vendor, ManufacturerSource::Signal)
                .await
                .map_err(AppError::Internal)?;
        }

        Ok(())
    }

    async fn record_signal_for_mac(
        &self,
        mac: &str,
        kind: DeviceSignalKind,
        value: &str,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let Some(device) = self
            .devices
            .find_by_mac(&mac.to_lowercase())
            .await
            .map_err(AppError::Internal)?
        else {
            tracing::debug!(%mac, ?kind, "identification: no device for MAC yet, dropping signal");
            return Ok(());
        };

        self.record_signal(&device.id.to_string(), kind, value)
            .await
    }

    async fn reconcile_from_catalog(&self) -> Result<usize, AppError> {
        auth_context::require_admin()?;

        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;

        let mut named = 0;
        for device in devices.iter().filter(|d| d.manufacturer.is_none()) {
            let Some(vendor) = vendor_catalog::lookup_oui_override(&device.mac) else {
                continue;
            };
            let updated = self
                .identification
                .set_manufacturer_if_absent(
                    &device.id.to_string(),
                    vendor,
                    ManufacturerSource::Catalog,
                )
                .await
                .map_err(AppError::Internal)?;
            if updated {
                named += 1;
                tracing::info!(
                    mac = %device.mac,
                    vendor,
                    "identification: named a previously-unidentified device from the vendor catalog"
                );
            }
        }
        Ok(named)
    }

    async fn signals_for(&self, device_id: &str) -> Result<Vec<DeviceSignal>, AppError> {
        auth_context::require_admin()?;
        self.identification
            .find_by_device(device_id)
            .await
            .map_err(AppError::Internal)
    }

    async fn probe_device(&self, device_id: &str) -> Result<ProbeOutcome, AppError> {
        auth_context::require_admin()?;

        let device = self
            .devices
            .find_by_id(device_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("device {device_id} not found")))?;

        // Both guards protect the same thing: that the address we contact still
        // belongs to the device whose row we are about to name. `last_ip` is
        // last-observation-wins, so a stale one points at whoever holds that
        // address now, and `set_manufacturer_if_absent` would write their
        // vendor onto this row permanently.
        let unseen_secs = Utc::now()
            .signed_duration_since(device.last_seen)
            .num_seconds();
        let departure_secs = i64::try_from(self.departure_timeout.as_secs()).unwrap_or(i64::MAX);
        if unseen_secs > departure_secs {
            return Err(AppError::Conflict(
                "device is not currently on the network".to_owned(),
            ));
        }

        let ip: IpAddr = device.last_ip.parse().map_err(|_| {
            AppError::Conflict(format!(
                "device has no usable address to probe: {}",
                device.last_ip
            ))
        })?;
        // Remote WireGuard peers live on 10.100.64.0/24, so they stay probeable;
        // a globally-routable address means the record has drifted into
        // something we have no business contacting.
        if !is_private_ip(ip) {
            return Err(AppError::Conflict(
                "device's last known address is not on a private network".to_owned(),
            ));
        }

        // The probe surface is the catalog's, and only the catalog's — see the
        // ADR 0025 §5 invariant on [`DeviceProber`].
        let ports_probed = vendor_catalog::probe_ports();
        let answering_ports = self.prober.probe(ip, &ports_probed).await;

        tracing::info!(
            %device_id,
            %ip,
            probed = ports_probed.len(),
            answered = answering_ports.len(),
            "identification: admin-triggered port probe complete"
        );

        // Funnel each hit through `record_signal` so vendor resolution, the
        // per-kind cap and first-writer-wins naming all come for free.
        for port in &answering_ports {
            self.record_signal(device_id, DeviceSignalKind::ProbedPort, &port.to_string())
                .await?;
        }

        Ok(ProbeOutcome {
            ports_probed,
            answering_ports,
        })
    }
}
