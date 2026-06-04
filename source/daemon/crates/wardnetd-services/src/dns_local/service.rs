//! Local-DNS CRUD service: authoritative zones, custom records, and
//! forwarding rules. See the [module docs](super) for scope.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use wardnet_common::api::{
    CreateForwardingRuleRequest, CreateForwardingRuleResponse, CreateRecordRequest,
    CreateRecordResponse, CreateZoneRequest, CreateZoneResponse, DeleteForwardingRuleResponse,
    DeleteRecordResponse, DeleteZoneResponse, GetForwardingRuleResponse, GetRecordResponse,
    GetZoneResponse, ListForwardingRulesResponse, ListRecordsResponse, ListZonesResponse,
    UpdateForwardingRuleRequest, UpdateForwardingRuleResponse, UpdateRecordRequest,
    UpdateRecordResponse, UpdateZoneRequest, UpdateZoneResponse,
};
use wardnet_common::dns::{ConditionalForwardingRule, CustomDnsRecord, DnsZone};
use wardnetd_data::repository::{
    DnsLocalRepository, RecordRow, RecordUpdate, RuleRow, RuleUpdate, ZoneRow, ZoneUpdate,
};

use crate::auth_context;
use crate::error::AppError;

#[async_trait]
pub trait DnsLocalService: Send + Sync {
    // ── Zones ───────────────────────────────────────────────────────────
    async fn list_zones(&self) -> Result<ListZonesResponse, AppError>;
    async fn get_zone(&self, id: Uuid) -> Result<GetZoneResponse, AppError>;
    async fn create_zone(&self, req: CreateZoneRequest) -> Result<CreateZoneResponse, AppError>;
    async fn update_zone(
        &self,
        id: Uuid,
        req: UpdateZoneRequest,
    ) -> Result<UpdateZoneResponse, AppError>;
    async fn delete_zone(&self, id: Uuid) -> Result<DeleteZoneResponse, AppError>;
    /// Records assigned to a zone. 404 if the zone does not exist.
    async fn list_zone_records(&self, zone_id: Uuid) -> Result<ListRecordsResponse, AppError>;

    // ── Records ─────────────────────────────────────────────────────────
    async fn list_records(&self) -> Result<ListRecordsResponse, AppError>;
    async fn get_record(&self, id: Uuid) -> Result<GetRecordResponse, AppError>;
    async fn create_record(
        &self,
        req: CreateRecordRequest,
    ) -> Result<CreateRecordResponse, AppError>;
    async fn update_record(
        &self,
        id: Uuid,
        req: UpdateRecordRequest,
    ) -> Result<UpdateRecordResponse, AppError>;
    async fn delete_record(&self, id: Uuid) -> Result<DeleteRecordResponse, AppError>;

    // ── Forwarding rules ────────────────────────────────────────────────
    async fn list_forwarding_rules(&self) -> Result<ListForwardingRulesResponse, AppError>;
    async fn get_forwarding_rule(&self, id: Uuid) -> Result<GetForwardingRuleResponse, AppError>;
    async fn create_forwarding_rule(
        &self,
        req: CreateForwardingRuleRequest,
    ) -> Result<CreateForwardingRuleResponse, AppError>;
    async fn update_forwarding_rule(
        &self,
        id: Uuid,
        req: UpdateForwardingRuleRequest,
    ) -> Result<UpdateForwardingRuleResponse, AppError>;
    async fn delete_forwarding_rule(
        &self,
        id: Uuid,
    ) -> Result<DeleteForwardingRuleResponse, AppError>;
}

pub struct DnsLocalServiceImpl {
    repo: Arc<dyn DnsLocalRepository>,
}

impl DnsLocalServiceImpl {
    #[must_use]
    pub fn new(repo: Arc<dyn DnsLocalRepository>) -> Self {
        Self { repo }
    }

    /// FQDN-style validation for record / forwarding domains (requires a dot).
    /// Mirrors `DnsFilterServiceImpl::validate_domain`.
    fn validate_domain(domain: &str) -> Result<(), AppError> {
        if domain.is_empty() {
            return Err(AppError::BadRequest("domain must not be empty".to_owned()));
        }
        if domain.len() > 253 {
            return Err(AppError::BadRequest(
                "domain must be <= 253 characters".to_owned(),
            ));
        }
        if !domain.contains('.') {
            return Err(AppError::BadRequest(
                "domain must contain at least one '.'".to_owned(),
            ));
        }
        Self::validate_charset(domain)
    }

    /// Zone names are single labels (e.g. `lan`, `home`) so, unlike record
    /// domains, they are NOT required to contain a dot.
    fn validate_zone_name(name: &str) -> Result<(), AppError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::BadRequest(
                "zone name must not be empty".to_owned(),
            ));
        }
        if name.len() > 253 {
            return Err(AppError::BadRequest(
                "zone name must be <= 253 characters".to_owned(),
            ));
        }
        Self::validate_charset(name)
    }

    fn validate_charset(s: &str) -> Result<(), AppError> {
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return Err(AppError::BadRequest(
                "value contains invalid characters (allowed: alphanumeric, '.', '-', '_')"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    fn require_non_empty(field: &str, value: &str) -> Result<(), AppError> {
        if value.trim().is_empty() {
            return Err(AppError::BadRequest(format!("{field} must not be empty")));
        }
        Ok(())
    }

    /// Map a repository error to an [`AppError`], translating a `SQLite`
    /// UNIQUE-constraint violation (duplicate zone name, `(domain,
    /// record_type)` pair, or forwarding domain) into a 409 `Conflict` rather
    /// than a 500. Mirrors the `anyhow → sqlx::Error` downcast in
    /// [`crate::db_busy::is_busy_error`].
    fn map_repo_err(err: anyhow::Error, conflict_msg: &str) -> AppError {
        if let Some(sqlx::Error::Database(db_err)) = err.downcast_ref::<sqlx::Error>()
            && db_err.is_unique_violation()
        {
            return AppError::Conflict(conflict_msg.to_owned());
        }
        AppError::Internal(err)
    }

    async fn ensure_zone(&self, id: Uuid) -> Result<DnsZone, AppError> {
        self.repo
            .get_zone(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("zone {id} not found")))
    }

    async fn ensure_record(&self, id: Uuid) -> Result<CustomDnsRecord, AppError> {
        self.repo
            .get_record(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("record {id} not found")))
    }

    async fn ensure_forwarding_rule(
        &self,
        id: Uuid,
    ) -> Result<ConditionalForwardingRule, AppError> {
        self.repo
            .get_rule(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("forwarding rule {id} not found")))
    }
}

#[async_trait]
impl DnsLocalService for DnsLocalServiceImpl {
    // ── Zones ───────────────────────────────────────────────────────────

    async fn list_zones(&self) -> Result<ListZonesResponse, AppError> {
        auth_context::require_admin()?;
        let zones = self.repo.list_zones().await.map_err(AppError::Internal)?;
        Ok(ListZonesResponse { zones })
    }

    async fn get_zone(&self, id: Uuid) -> Result<GetZoneResponse, AppError> {
        auth_context::require_admin()?;
        let zone = self.ensure_zone(id).await?;
        Ok(GetZoneResponse { zone })
    }

    async fn create_zone(&self, req: CreateZoneRequest) -> Result<CreateZoneResponse, AppError> {
        auth_context::require_admin()?;
        Self::validate_zone_name(&req.name)?;
        let row = ZoneRow {
            id: Uuid::new_v4().to_string(),
            name: req.name,
            enabled: req.enabled,
        };
        let zone = self
            .repo
            .create_zone(&row)
            .await
            .map_err(|e| Self::map_repo_err(e, "a zone with this name already exists"))?;
        Ok(CreateZoneResponse {
            zone,
            message: "zone created".to_owned(),
        })
    }

    async fn update_zone(
        &self,
        id: Uuid,
        req: UpdateZoneRequest,
    ) -> Result<UpdateZoneResponse, AppError> {
        auth_context::require_admin()?;
        // `update_zone` returns Ok(true) even for a missing id, so check first.
        self.ensure_zone(id).await?;
        if let Some(name) = req.name.as_deref() {
            Self::validate_zone_name(name)?;
        }
        let update = ZoneUpdate {
            name: req.name,
            enabled: req.enabled,
        };
        self.repo
            .update_zone(id, &update)
            .await
            .map_err(|e| Self::map_repo_err(e, "a zone with this name already exists"))?;
        let zone = self.ensure_zone(id).await?;
        Ok(UpdateZoneResponse {
            zone,
            message: "zone updated".to_owned(),
        })
    }

    async fn delete_zone(&self, id: Uuid) -> Result<DeleteZoneResponse, AppError> {
        auth_context::require_admin()?;
        let deleted = self
            .repo
            .delete_zone(id)
            .await
            .map_err(AppError::Internal)?;
        if !deleted {
            return Err(AppError::NotFound(format!("zone {id} not found")));
        }
        Ok(DeleteZoneResponse {
            message: format!("zone {id} deleted"),
        })
    }

    async fn list_zone_records(&self, zone_id: Uuid) -> Result<ListRecordsResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_zone(zone_id).await?;
        let records = self
            .repo
            .list_records_by_zone(zone_id)
            .await
            .map_err(AppError::Internal)?;
        Ok(ListRecordsResponse { records })
    }

    // ── Records ─────────────────────────────────────────────────────────

    async fn list_records(&self) -> Result<ListRecordsResponse, AppError> {
        auth_context::require_admin()?;
        let records = self.repo.list_records().await.map_err(AppError::Internal)?;
        Ok(ListRecordsResponse { records })
    }

    async fn get_record(&self, id: Uuid) -> Result<GetRecordResponse, AppError> {
        auth_context::require_admin()?;
        let record = self.ensure_record(id).await?;
        Ok(GetRecordResponse { record })
    }

    async fn create_record(
        &self,
        req: CreateRecordRequest,
    ) -> Result<CreateRecordResponse, AppError> {
        auth_context::require_admin()?;
        Self::validate_domain(&req.domain)?;
        Self::require_non_empty("value", &req.value)?;
        if let Some(zone_id) = req.zone_id {
            self.ensure_zone(zone_id).await?;
        }
        let row = RecordRow {
            id: Uuid::new_v4().to_string(),
            zone_id: req.zone_id,
            domain: req.domain,
            record_type: req.record_type,
            value: req.value,
            ttl: req.ttl,
            enabled: req.enabled,
        };
        let record = self.repo.create_record(&row).await.map_err(|e| {
            Self::map_repo_err(e, "a record with this domain and type already exists")
        })?;
        Ok(CreateRecordResponse {
            record,
            message: "record created".to_owned(),
        })
    }

    async fn update_record(
        &self,
        id: Uuid,
        req: UpdateRecordRequest,
    ) -> Result<UpdateRecordResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_record(id).await?;
        if let Some(domain) = req.domain.as_deref() {
            Self::validate_domain(domain)?;
        }
        if let Some(value) = req.value.as_deref() {
            Self::require_non_empty("value", value)?;
        }
        // Reassigning to a zone: that zone must exist. `Some(None)` clears the
        // link and `None` leaves it untouched — neither needs a check.
        if let Some(Some(zone_id)) = req.zone_id {
            self.ensure_zone(zone_id).await?;
        }
        let update = RecordUpdate {
            zone_id: req.zone_id,
            domain: req.domain,
            record_type: req.record_type,
            value: req.value,
            ttl: req.ttl,
            enabled: req.enabled,
        };
        self.repo.update_record(id, &update).await.map_err(|e| {
            Self::map_repo_err(e, "a record with this domain and type already exists")
        })?;
        let record = self.ensure_record(id).await?;
        Ok(UpdateRecordResponse {
            record,
            message: "record updated".to_owned(),
        })
    }

    async fn delete_record(&self, id: Uuid) -> Result<DeleteRecordResponse, AppError> {
        auth_context::require_admin()?;
        let deleted = self
            .repo
            .delete_record(id)
            .await
            .map_err(AppError::Internal)?;
        if !deleted {
            return Err(AppError::NotFound(format!("record {id} not found")));
        }
        Ok(DeleteRecordResponse {
            message: format!("record {id} deleted"),
        })
    }

    // ── Forwarding rules ────────────────────────────────────────────────

    async fn list_forwarding_rules(&self) -> Result<ListForwardingRulesResponse, AppError> {
        auth_context::require_admin()?;
        let rules = self.repo.list_rules().await.map_err(AppError::Internal)?;
        Ok(ListForwardingRulesResponse { rules })
    }

    async fn get_forwarding_rule(&self, id: Uuid) -> Result<GetForwardingRuleResponse, AppError> {
        auth_context::require_admin()?;
        let rule = self.ensure_forwarding_rule(id).await?;
        Ok(GetForwardingRuleResponse { rule })
    }

    async fn create_forwarding_rule(
        &self,
        req: CreateForwardingRuleRequest,
    ) -> Result<CreateForwardingRuleResponse, AppError> {
        auth_context::require_admin()?;
        Self::validate_domain(&req.domain)?;
        Self::require_non_empty("upstream", &req.upstream)?;
        let row = RuleRow {
            id: Uuid::new_v4().to_string(),
            domain: req.domain,
            upstream: req.upstream,
            enabled: req.enabled,
        };
        let rule = self.repo.create_rule(&row).await.map_err(|e| {
            Self::map_repo_err(e, "a forwarding rule for this domain already exists")
        })?;
        Ok(CreateForwardingRuleResponse {
            rule,
            message: "forwarding rule created".to_owned(),
        })
    }

    async fn update_forwarding_rule(
        &self,
        id: Uuid,
        req: UpdateForwardingRuleRequest,
    ) -> Result<UpdateForwardingRuleResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_forwarding_rule(id).await?;
        if let Some(domain) = req.domain.as_deref() {
            Self::validate_domain(domain)?;
        }
        if let Some(upstream) = req.upstream.as_deref() {
            Self::require_non_empty("upstream", upstream)?;
        }
        let update = RuleUpdate {
            domain: req.domain,
            upstream: req.upstream,
            enabled: req.enabled,
        };
        self.repo.update_rule(id, &update).await.map_err(|e| {
            Self::map_repo_err(e, "a forwarding rule for this domain already exists")
        })?;
        let rule = self.ensure_forwarding_rule(id).await?;
        Ok(UpdateForwardingRuleResponse {
            rule,
            message: "forwarding rule updated".to_owned(),
        })
    }

    async fn delete_forwarding_rule(
        &self,
        id: Uuid,
    ) -> Result<DeleteForwardingRuleResponse, AppError> {
        auth_context::require_admin()?;
        let deleted = self
            .repo
            .delete_rule(id)
            .await
            .map_err(AppError::Internal)?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "forwarding rule {id} not found"
            )));
        }
        Ok(DeleteForwardingRuleResponse {
            message: format!("forwarding rule {id} deleted"),
        })
    }
}
