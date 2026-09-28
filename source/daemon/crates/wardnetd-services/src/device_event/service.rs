use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;
use wardnet_common::device_event::{DeviceEvent, DeviceEventKind};
use wardnetd_data::repository::{
    DEVICE_EVENT_MAX_PER_DEVICE, DEVICE_EVENT_RETENTION_DAYS, DeviceEventRepository,
    DeviceRepository, NewDeviceEvent,
};

use crate::auth_context;
use crate::error::AppError;

/// How the event bus identified the device.
///
/// Most events carry a device id; the conntrack flush sites work in terms of an
/// address, because that is what a firewall rule is keyed by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceRef {
    Id(Uuid),
    Ip(String),
}

/// What kind of event to record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentKind {
    /// The kind is known from the event alone.
    Fixed(DeviceEventKind),
    /// A device appeared. Whether that is a first sighting or a return is
    /// decided from what the timeline has already recorded — the bus publishes
    /// `DeviceDiscovered` for both, and three other listeners depend on that,
    /// so the bus is deliberately left as it is.
    Arrival,
}

/// One event the listener wants recorded, before the service resolves the
/// pieces the bus did not carry.
#[derive(Debug, Clone)]
pub struct DeviceEventIntent {
    pub device: DeviceRef,
    /// Present when the event carried it; otherwise resolved from the device.
    pub mac: Option<String>,
    pub kind: IntentKind,
    pub details: Option<serde_json::Value>,
    pub at: DateTime<Utc>,
}

/// Reads and writes the observational device event log.
#[async_trait]
pub trait DeviceEventService: Send + Sync {
    /// Resolve and append one event. Requires admin auth context.
    ///
    /// An intent naming a device that no longer exists is dropped rather than
    /// failing: a flush can race a device being pruned, and losing one timeline
    /// row is preferable to a listener that logs an error every time.
    async fn record(&self, intent: DeviceEventIntent) -> Result<(), AppError>;

    /// Every event for one device inside `[from, to]`, oldest first.
    async fn list_for_device(
        &self,
        device_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DeviceEvent>, AppError>;

    /// How many events of `kind` one MAC recorded since `since`.
    async fn count_for_mac_since(
        &self,
        mac: &str,
        kind: DeviceEventKind,
        since: DateTime<Utc>,
    ) -> Result<i64, AppError>;

    /// Counts of `kind` per MAC since `since`, for the churn detector's sweep.
    async fn count_by_mac_since(
        &self,
        kind: DeviceEventKind,
        since: DateTime<Utc>,
    ) -> Result<Vec<(String, i64)>, AppError>;

    /// Apply both retention caps, returning how many rows were deleted.
    async fn prune(&self) -> Result<u64, AppError>;
}

pub struct DeviceEventServiceImpl {
    events: Arc<dyn DeviceEventRepository>,
    devices: Arc<dyn DeviceRepository>,
}

impl DeviceEventServiceImpl {
    #[must_use]
    pub fn new(events: Arc<dyn DeviceEventRepository>, devices: Arc<dyn DeviceRepository>) -> Self {
        Self { events, devices }
    }

    /// Resolve the intent's device reference to `(device_id, mac)`.
    async fn resolve(
        &self,
        intent: &DeviceEventIntent,
    ) -> Result<Option<(String, String)>, AppError> {
        match &intent.device {
            DeviceRef::Id(id) => {
                let id = id.to_string();
                if let Some(mac) = intent.mac.clone() {
                    return Ok(Some((id, mac)));
                }
                let found = self
                    .devices
                    .find_by_id(&id)
                    .await
                    .map_err(AppError::Internal)?;
                Ok(found.map(|device| (id, device.mac)))
            }
            DeviceRef::Ip(ip) => {
                let found = self
                    .devices
                    .find_by_ip(ip)
                    .await
                    .map_err(AppError::Internal)?;
                Ok(found.map(|device| (device.id.to_string(), device.mac)))
            }
        }
    }
}

#[async_trait]
impl DeviceEventService for DeviceEventServiceImpl {
    async fn record(&self, intent: DeviceEventIntent) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let Some((device_id, mac)) = self.resolve(&intent).await? else {
            return Ok(());
        };

        let kind = match intent.kind {
            IntentKind::Fixed(kind) => kind,
            IntentKind::Arrival => {
                // An arrival following a departure is a return; anything else —
                // including a device with no history at all — is a first
                // sighting.
                let latest = self
                    .events
                    .latest_kind_for_device(&device_id)
                    .await
                    .map_err(AppError::Internal)?;
                if latest == Some(DeviceEventKind::Gone) {
                    DeviceEventKind::Returned
                } else {
                    DeviceEventKind::Discovered
                }
            }
        };

        let details = intent.details.as_ref().map(ToString::to_string);
        self.events
            .record(NewDeviceEvent {
                device_id: &device_id,
                mac: &mac,
                kind,
                details: details.as_deref(),
                created_at: intent.at,
            })
            .await
            .map_err(AppError::Internal)
    }

    async fn list_for_device(
        &self,
        device_id: Uuid,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DeviceEvent>, AppError> {
        auth_context::require_admin()?;
        self.events
            .list_for_device(&device_id.to_string(), from, to)
            .await
            .map_err(AppError::Internal)
    }

    async fn count_for_mac_since(
        &self,
        mac: &str,
        kind: DeviceEventKind,
        since: DateTime<Utc>,
    ) -> Result<i64, AppError> {
        auth_context::require_admin()?;
        self.events
            .count_for_mac_since(mac, kind, since)
            .await
            .map_err(AppError::Internal)
    }

    async fn count_by_mac_since(
        &self,
        kind: DeviceEventKind,
        since: DateTime<Utc>,
    ) -> Result<Vec<(String, i64)>, AppError> {
        auth_context::require_admin()?;
        self.events
            .count_by_mac_since(kind, since)
            .await
            .map_err(AppError::Internal)
    }

    async fn prune(&self) -> Result<u64, AppError> {
        auth_context::require_admin()?;
        let cutoff = Utc::now() - Duration::days(DEVICE_EVENT_RETENTION_DAYS);
        self.events
            .prune(cutoff, DEVICE_EVENT_MAX_PER_DEVICE)
            .await
            .map_err(AppError::Internal)
    }
}
