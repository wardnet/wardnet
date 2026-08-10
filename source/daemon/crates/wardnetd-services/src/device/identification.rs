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

use async_trait::async_trait;
use wardnet_common::device::{DeviceSignal, DeviceSignalKind, ManufacturerSource};

use crate::auth_context;
use crate::error::AppError;
use wardnetd_data::repository::{
    DeviceIdentificationRepository, DeviceRepository, DeviceSignalRow,
};
use wardnetd_data::vendor_catalog;

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

    /// Record a signal against whichever device currently holds `ip`.
    ///
    /// The mDNS observer resolves a browse result to a device by its last
    /// observed IP, since a service advertisement carries an address, not a
    /// MAC. `last_ip` is not unique across the devices table — a departed
    /// device keeps its row until discovery clears it — so an address can
    /// transiently map to more than one device. Rather than guess (ADR 0025,
    /// decision on mDNS: a wrong attribution is worse than no signal), an IP
    /// that resolves to zero or to more than one device is skipped with a debug
    /// log and `Ok(())` is returned.
    async fn record_signal_for_ip(
        &self,
        ip: IpAddr,
        kind: DeviceSignalKind,
        value: &str,
    ) -> Result<(), AppError>;

    /// Every signal recorded for a device, most recent first.
    async fn signals_for(&self, device_id: &str) -> Result<Vec<DeviceSignal>, AppError>;

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
}

impl DeviceIdentificationServiceImpl {
    #[must_use]
    pub fn new(
        identification: Arc<dyn DeviceIdentificationRepository>,
        devices: Arc<dyn DeviceRepository>,
    ) -> Self {
        Self {
            identification,
            devices,
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

    async fn record_signal_for_ip(
        &self,
        ip: IpAddr,
        kind: DeviceSignalKind,
        value: &str,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;

        // `last_ip` is not unique: a departed device keeps its row until
        // discovery clears the address, so DHCP recycling an IP to a new device
        // leaves two rows sharing it. `find_by_ip` resolves that by taking the
        // most-recently-seen row — a reasonable guess for the DNS attribution
        // path, but the wrong call here. An mDNS advertisement that names a
        // vendor would stamp that name on whichever device won the tie, and a
        // confident-but-wrong manufacturer is the exact failure #1099 was filed
        // about. So resolve the full set (an indexed `last_ip` lookup) and
        // attribute only when exactly one device claims the address; zero or
        // many is ambiguous and skipped.
        let matches = self
            .devices
            .find_all_by_ip(&ip.to_string())
            .await
            .map_err(AppError::Internal)?;

        let [device] = matches.as_slice() else {
            let match_count = matches.len();
            tracing::debug!(
                %ip,
                ?kind,
                match_count,
                "identification: IP {ip} maps to {match_count} devices, skipping ambiguous mDNS {kind:?} attribution",
            );
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
}
