use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{CreateZoneExceptionRequest, UpdateZoneExceptionRequest};
use wardnet_common::event::WardnetEvent;
use wardnet_common::zone_exception::{
    ExceptionEndpoint, ExceptionEndpointKind, ServiceSpec, ZoneException,
};
use wardnetd_data::repository::{DeviceRepository, NetworkZoneRepository, ZoneExceptionRepository};

use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;

/// Cross-zone exceptions service (epic #244, issue #737).
///
/// CRUD over the exception catalog — admin-granted allowances for one endpoint
/// to reach another across an otherwise-isolated zone boundary (e.g. a phone
/// casting to a TV). Every method is admin-gated. Endpoint existence and port
/// validation live here; enforcement is a later commit that consumes the
/// [`WardnetEvent::ZoneExceptionsChanged`] this service emits.
#[async_trait]
pub trait ZoneExceptionService: Send + Sync {
    /// List all exceptions.
    async fn list_exceptions(&self) -> Result<Vec<ZoneException>, AppError>;

    /// Fetch a single exception.
    async fn get_exception(&self, id: Uuid) -> Result<ZoneException, AppError>;

    /// Create a new exception.
    async fn create_exception(
        &self,
        req: CreateZoneExceptionRequest,
    ) -> Result<ZoneException, AppError>;

    /// Partially update an exception. Absent fields are left unchanged.
    async fn update_exception(
        &self,
        id: Uuid,
        req: UpdateZoneExceptionRequest,
    ) -> Result<ZoneException, AppError>;

    /// Delete an exception.
    async fn delete_exception(&self, id: Uuid) -> Result<(), AppError>;
}

/// Default implementation of [`ZoneExceptionService`].
pub struct ZoneExceptionServiceImpl {
    exceptions: Arc<dyn ZoneExceptionRepository>,
    zones: Arc<dyn NetworkZoneRepository>,
    devices: Arc<dyn DeviceRepository>,
    events: Arc<dyn EventPublisher>,
}

impl ZoneExceptionServiceImpl {
    /// Create a new service backed by the given repositories and event publisher.
    #[must_use]
    pub fn new(
        exceptions: Arc<dyn ZoneExceptionRepository>,
        zones: Arc<dyn NetworkZoneRepository>,
        devices: Arc<dyn DeviceRepository>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            exceptions,
            zones,
            devices,
            events,
        }
    }

    /// Fetch an exception by id or return `NotFound`.
    async fn require_exception(&self, id: Uuid) -> Result<ZoneException, AppError> {
        self.exceptions
            .find_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("zone exception {id} not found")))
    }

    /// Verify an endpoint's referenced device or zone exists.
    async fn assert_endpoint_exists(&self, endpoint: &ExceptionEndpoint) -> Result<(), AppError> {
        let exists = match endpoint.kind {
            ExceptionEndpointKind::Device => self
                .devices
                .find_by_id(&endpoint.id.to_string())
                .await
                .map_err(AppError::Internal)?
                .is_some(),
            ExceptionEndpointKind::Zone => self
                .zones
                .find_by_id(&endpoint.id.to_string())
                .await
                .map_err(AppError::Internal)?
                .is_some(),
        };
        if !exists {
            return Err(AppError::BadRequest(format!(
                "endpoint {} {} does not exist",
                match endpoint.kind {
                    ExceptionEndpointKind::Device => "device",
                    ExceptionEndpointKind::Zone => "zone",
                },
                endpoint.id
            )));
        }
        Ok(())
    }

    /// Validate a fully-resolved exception's endpoints and service spec.
    async fn validate(
        &self,
        from: &ExceptionEndpoint,
        to: &ExceptionEndpoint,
        service: &ServiceSpec,
    ) -> Result<(), AppError> {
        if from == to {
            return Err(AppError::BadRequest(
                "an exception's from and to endpoints must differ".to_owned(),
            ));
        }
        // A full-range (all-ports) exception opens the entire port space, so it
        // is only safe between two specific devices — never a whole zone. Keyed
        // on the RESOLVED ports so it covers the Mirroring preset (which resolves
        // to 1-65535) AND an explicit `Ports { [1-65535] }` spec that would
        // otherwise slip past a preset-only guard.
        let wide_open = service
            .resolve_ports()
            .iter()
            .any(|p| p.from <= 1 && p.to == u16::MAX);
        if wide_open
            && (from.kind != ExceptionEndpointKind::Device
                || to.kind != ExceptionEndpointKind::Device)
        {
            return Err(AppError::BadRequest(
                "a full-range (all-ports) exception requires device-to-device endpoints".to_owned(),
            ));
        }
        self.assert_endpoint_exists(from).await?;
        self.assert_endpoint_exists(to).await?;
        Self::validate_service(service)?;
        Ok(())
    }

    /// Validate an explicit port list: non-empty, each `from <= to`, `from >= 1`.
    /// Presets are always valid (curated by the daemon).
    fn validate_service(service: &ServiceSpec) -> Result<(), AppError> {
        if let ServiceSpec::Ports { ports } = service {
            if ports.is_empty() {
                return Err(AppError::BadRequest(
                    "custom port list must not be empty".to_owned(),
                ));
            }
            for port in ports {
                if port.from < 1 || port.from > port.to {
                    return Err(AppError::BadRequest(format!(
                        "invalid port range {}-{}: from must be >= 1 and <= to",
                        port.from, port.to
                    )));
                }
            }
        }
        Ok(())
    }

    fn emit_changed(&self) {
        self.events.publish(WardnetEvent::ZoneExceptionsChanged {
            timestamp: chrono::Utc::now(),
        });
    }
}

#[async_trait]
impl ZoneExceptionService for ZoneExceptionServiceImpl {
    async fn list_exceptions(&self) -> Result<Vec<ZoneException>, AppError> {
        auth_context::require_admin()?;
        self.exceptions.find_all().await.map_err(AppError::Internal)
    }

    async fn get_exception(&self, id: Uuid) -> Result<ZoneException, AppError> {
        auth_context::require_admin()?;
        self.require_exception(id).await
    }

    async fn create_exception(
        &self,
        req: CreateZoneExceptionRequest,
    ) -> Result<ZoneException, AppError> {
        auth_context::require_admin()?;
        self.validate(&req.from, &req.to, &req.service).await?;

        let now = chrono::Utc::now();
        let exception = ZoneException {
            id: Uuid::new_v4(),
            from: req.from,
            to: req.to,
            service: req.service,
            bidirectional: req.bidirectional,
            created_at: now,
            updated_at: now,
        };
        self.exceptions
            .insert(&exception)
            .await
            .map_err(AppError::Internal)?;

        // An exception naming a device is an admin configuration act on that
        // device, so it promotes to managed (issue #1181). Without this the
        // retention prune could delete a device 30 days after it was last seen
        // while this MAC-independent, FK-less exception row still referenced
        // it — leaving a rule that grants cross-zone reach to whatever device
        // next claims that id. Zone endpoints promote nothing.
        for endpoint in [&exception.from, &exception.to] {
            if endpoint.kind == ExceptionEndpointKind::Device {
                self.devices
                    .set_managed(&endpoint.id.to_string(), true)
                    .await
                    .map_err(AppError::Internal)?;
            }
        }

        self.emit_changed();
        Ok(exception)
    }

    async fn update_exception(
        &self,
        id: Uuid,
        req: UpdateZoneExceptionRequest,
    ) -> Result<ZoneException, AppError> {
        auth_context::require_admin()?;
        let mut exception = self.require_exception(id).await?;

        if let Some(from) = req.from {
            exception.from = from;
        }
        if let Some(to) = req.to {
            exception.to = to;
        }
        if let Some(service) = req.service {
            exception.service = service;
        }
        if let Some(bidirectional) = req.bidirectional {
            exception.bidirectional = bidirectional;
        }

        self.validate(&exception.from, &exception.to, &exception.service)
            .await?;

        exception.updated_at = chrono::Utc::now();
        self.exceptions
            .update(&exception)
            .await
            .map_err(AppError::Internal)?;
        self.emit_changed();
        Ok(exception)
    }

    async fn delete_exception(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.require_exception(id).await?;
        self.exceptions
            .delete(&id.to_string())
            .await
            .map_err(AppError::Internal)?;
        self.emit_changed();
        Ok(())
    }
}
