use async_trait::async_trait;
use wardnet_common::device::{DeviceSignal, DeviceSignalKind, ManufacturerSource};

/// Row data for recording an observed identification signal.
pub struct DeviceSignalRow {
    pub device_id: String,
    pub kind: DeviceSignalKind,
    pub value: String,
    /// `true` when we matched the raw observation against the curated vendor
    /// catalog rather than simply recording what the device told us.
    pub inferred: bool,
}

/// Data access for device identification (issue #1099) — the observed signals
/// that help name a device, plus the manufacturer write those signals produce.
///
/// Deliberately a separate, narrow trait rather than extra methods on
/// [`DeviceRepository`](crate::repository::DeviceRepository). Identification is
/// a self-contained concern with exactly one producer path and one consumer,
/// and keeping it here means the ten existing `DeviceRepository` mocks do not
/// have to grow a method none of them exercise.
#[async_trait]
pub trait DeviceIdentificationRepository: Send + Sync {
    /// Record an observed signal, replacing any previous observation of the
    /// same `(device, kind, value)` so the timestamp reflects the latest
    /// sighting. Repeated observations of an unchanged fact are the normal
    /// case — a device re-announcing the same mDNS service on every renewal —
    /// so this must not accumulate duplicates.
    async fn record(&self, row: &DeviceSignalRow) -> anyhow::Result<()>;

    /// All signals recorded for a device, most recently observed first.
    async fn find_by_device(&self, device_id: &str) -> anyhow::Result<Vec<DeviceSignal>>;

    /// Keep only the `keep` most recently observed values for one device and
    /// signal kind, discarding the rest.
    ///
    /// Signals arrive from an unauthenticated LAN path (DHCP), so a client that
    /// varies its vendor class on every renewal would otherwise append a new
    /// permanent row each time — the natural key includes `value`, so the
    /// upsert cannot collapse them.
    async fn prune_signals(
        &self,
        device_id: &str,
        kind: DeviceSignalKind,
        keep: usize,
    ) -> anyhow::Result<()>;

    /// Name a device that has no manufacturer yet, recording where the name
    /// came from. A device that already has one is left untouched.
    ///
    /// Conditional in SQL rather than read-then-write on purpose. A device
    /// commonly advertises several services from different vendors — an Apple
    /// TV answers both `_airplay._tcp` and `_googlecast._tcp` — and signals
    /// arrive concurrently from DHCP, mDNS and probes. Checking in the caller
    /// would leave a race that degrades the intended first-writer-wins into
    /// last-writer-wins, flipping a device's manufacturer on re-announcement.
    ///
    /// Returns whether a row was actually named.
    async fn set_manufacturer_if_absent(
        &self,
        device_id: &str,
        manufacturer: &str,
        source: ManufacturerSource,
    ) -> anyhow::Result<bool>;
}
