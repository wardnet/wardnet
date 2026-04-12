use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_types::api::{
    CreateDhcpReservationRequest, CreateDhcpReservationResponse, DeleteDhcpReservationResponse,
    DhcpConfigResponse, DhcpStatusResponse, ListDhcpLeasesResponse, ListDhcpReservationsResponse,
    RevokeDhcpLeaseResponse, ToggleDhcpRequest, UpdateDhcpConfigRequest,
};
use wardnet_types::dhcp::{DhcpConfig, DhcpLease, DhcpLeaseStatus};

use crate::auth_context;
use crate::error::AppError;
use crate::repository::SystemConfigRepository;
use crate::repository::{DhcpLeaseLogRow, DhcpLeaseRow, DhcpRepository, DhcpReservationRow};

/// DHCP lease and reservation management.
///
/// Handles DHCP configuration, lease lifecycle, and static reservations.
/// All operations require admin authentication.
#[async_trait]
pub trait DhcpService: Send + Sync {
    /// Get the current DHCP configuration.
    async fn get_config(&self) -> Result<DhcpConfigResponse, AppError>;

    /// Update the DHCP pool configuration.
    async fn update_config(
        &self,
        req: UpdateDhcpConfigRequest,
    ) -> Result<DhcpConfigResponse, AppError>;

    /// Enable or disable the DHCP server.
    async fn toggle(&self, req: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError>;

    /// List all active DHCP leases.
    async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError>;

    /// Revoke an active lease.
    async fn revoke_lease(&self, id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError>;

    /// List all static reservations.
    async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError>;

    /// Create a new static reservation.
    async fn create_reservation(
        &self,
        req: CreateDhcpReservationRequest,
    ) -> Result<CreateDhcpReservationResponse, AppError>;

    /// Delete a static reservation.
    async fn delete_reservation(&self, id: Uuid)
    -> Result<DeleteDhcpReservationResponse, AppError>;

    /// Get DHCP server status (running, pool usage).
    async fn status(&self) -> Result<DhcpStatusResponse, AppError>;

    // ── Runtime methods (called by the DHCP server, not HTTP handlers) ──

    /// Assign a lease for a DHCP DISCOVER -- used by the DHCP server runtime.
    ///
    /// Checks reservations first (by MAC), otherwise allocates the first
    /// available IP in the pool range. Requires admin auth context.
    async fn assign_lease(&self, mac: &str, hostname: Option<&str>) -> Result<DhcpLease, AppError>;

    /// Renew/confirm a lease for a DHCP REQUEST -- used by the DHCP server runtime.
    ///
    /// Extends the existing lease if one is active, otherwise assigns a new one.
    /// Requires admin auth context.
    async fn renew_lease(&self, mac: &str) -> Result<DhcpLease, AppError>;

    /// Release a lease for a DHCP RELEASE -- used by the DHCP server runtime.
    ///
    /// Marks the active lease for the given MAC as released.
    /// Requires admin auth context.
    async fn release_lease(&self, mac: &str) -> Result<(), AppError>;

    /// Expire all stale leases whose `lease_end` is in the past.
    ///
    /// Called periodically by the DHCP runner. Returns the number of expired leases.
    /// Requires admin auth context.
    async fn cleanup_expired(&self) -> Result<u64, AppError>;

    /// Load the current DHCP configuration (public for the DHCP server runtime).
    ///
    /// Requires admin auth context.
    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError>;
}

/// Default implementation of [`DhcpService`].
pub struct DhcpServiceImpl {
    dhcp: Arc<dyn DhcpRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    /// Wardnet's own LAN IP, auto-detected at startup.
    gateway_ip: Ipv4Addr,
}

impl DhcpServiceImpl {
    /// Create a new DHCP service with the given dependencies.
    pub fn new(
        dhcp: Arc<dyn DhcpRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        gateway_ip: Ipv4Addr,
    ) -> Self {
        Self {
            dhcp,
            system_config,
            gateway_ip,
        }
    }

    /// Load the current DHCP configuration from `system_config`.
    async fn load_config(&self) -> Result<DhcpConfig, AppError> {
        // Derive subnet-aware defaults from the detected gateway IP.
        let gw = self.gateway_ip.octets();
        let default_pool_start = format!("{}.{}.{}.100", gw[0], gw[1], gw[2]);
        let default_pool_end = format!("{}.{}.{}.250", gw[0], gw[1], gw[2]);

        let enabled = self
            .system_config
            .get("dhcp_enabled")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "false".to_owned())
            == "true";

        let pool_start: Ipv4Addr = self
            .system_config
            .get("dhcp_pool_start")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or(default_pool_start)
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid pool_start: {e}")))?;

        let pool_end: Ipv4Addr = self
            .system_config
            .get("dhcp_pool_end")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or(default_pool_end)
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid pool_end: {e}")))?;

        let subnet_mask: Ipv4Addr = self
            .system_config
            .get("dhcp_subnet_mask")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "255.255.255.0".to_owned())
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid subnet_mask: {e}")))?;

        let upstream_dns_json = self
            .system_config
            .get("dhcp_upstream_dns")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| r#"["1.1.1.1","8.8.8.8"]"#.to_owned());
        let upstream_dns: Vec<Ipv4Addr> = serde_json::from_str::<Vec<String>>(&upstream_dns_json)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid upstream_dns: {e}")))?
            .iter()
            .map(|s| s.parse())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid upstream_dns IP: {e}")))?;

        let lease_duration_secs: u32 = self
            .system_config
            .get("dhcp_lease_duration_secs")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "86400".to_owned())
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid lease_duration_secs: {e}")))?;

        let router_ip_str = self
            .system_config
            .get("dhcp_router_ip")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_default();
        let router_ip = if router_ip_str.is_empty() {
            None
        } else {
            Some(
                router_ip_str
                    .parse()
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid router_ip: {e}")))?,
            )
        };

        Ok(DhcpConfig {
            enabled,
            gateway_ip: self.gateway_ip,
            pool_start,
            pool_end,
            subnet_mask,
            upstream_dns,
            lease_duration_secs,
            router_ip,
        })
    }

    /// Compute the total number of IPs in the pool.
    fn pool_size(start: Ipv4Addr, end: Ipv4Addr) -> u64 {
        let s = u32::from(start);
        let e = u32::from(end);
        if e >= s { u64::from(e - s + 1) } else { 0 }
    }

    /// Find the first available IP in the DHCP pool range that is not
    /// currently assigned to an active lease or a static reservation.
    async fn find_available_ip(&self, config: &DhcpConfig) -> Result<Ipv4Addr, AppError> {
        let active_leases = self
            .dhcp
            .list_active_leases()
            .await
            .map_err(AppError::Internal)?;
        let reservations = self
            .dhcp
            .list_reservations()
            .await
            .map_err(AppError::Internal)?;

        let used_ips: std::collections::HashSet<Ipv4Addr> = active_leases
            .iter()
            .map(|l| l.ip_address)
            .chain(reservations.iter().map(|r| r.ip_address))
            .collect();

        let start = u32::from(config.pool_start);
        let end = u32::from(config.pool_end);

        for ip_num in start..=end {
            let candidate = Ipv4Addr::from(ip_num);
            if !used_ips.contains(&candidate) {
                return Ok(candidate);
            }
        }

        Err(AppError::Conflict(
            "DHCP pool exhausted — no available IP addresses".to_owned(),
        ))
    }
}

#[async_trait]
impl DhcpService for DhcpServiceImpl {
    async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
        auth_context::require_admin()?;
        let config = self.load_config().await?;
        Ok(DhcpConfigResponse { config })
    }

    async fn update_config(
        &self,
        req: UpdateDhcpConfigRequest,
    ) -> Result<DhcpConfigResponse, AppError> {
        auth_context::require_admin()?;

        // Validate IP addresses.
        let pool_start: Ipv4Addr = req
            .pool_start
            .parse()
            .map_err(|_| AppError::BadRequest("invalid pool_start IP address".to_owned()))?;
        let pool_end: Ipv4Addr = req
            .pool_end
            .parse()
            .map_err(|_| AppError::BadRequest("invalid pool_end IP address".to_owned()))?;
        let _subnet_mask: Ipv4Addr = req
            .subnet_mask
            .parse()
            .map_err(|_| AppError::BadRequest("invalid subnet_mask IP address".to_owned()))?;

        if u32::from(pool_end) < u32::from(pool_start) {
            return Err(AppError::BadRequest(
                "pool_end must be >= pool_start".to_owned(),
            ));
        }

        for dns in &req.upstream_dns {
            let _: Ipv4Addr = dns.parse().map_err(|_| {
                AppError::BadRequest(format!("invalid upstream DNS address: {dns}"))
            })?;
        }

        if let Some(ref router_ip) = req.router_ip {
            let _: Ipv4Addr = router_ip
                .parse()
                .map_err(|_| AppError::BadRequest("invalid router_ip address".to_owned()))?;
        }

        // Store validated config.
        self.system_config
            .set("dhcp_pool_start", &req.pool_start)
            .await
            .map_err(AppError::Internal)?;
        self.system_config
            .set("dhcp_pool_end", &req.pool_end)
            .await
            .map_err(AppError::Internal)?;
        self.system_config
            .set("dhcp_subnet_mask", &req.subnet_mask)
            .await
            .map_err(AppError::Internal)?;
        let dns_json =
            serde_json::to_string(&req.upstream_dns).map_err(|e| AppError::Internal(e.into()))?;
        self.system_config
            .set("dhcp_upstream_dns", &dns_json)
            .await
            .map_err(AppError::Internal)?;
        self.system_config
            .set(
                "dhcp_lease_duration_secs",
                &req.lease_duration_secs.to_string(),
            )
            .await
            .map_err(AppError::Internal)?;
        self.system_config
            .set("dhcp_router_ip", req.router_ip.as_deref().unwrap_or(""))
            .await
            .map_err(AppError::Internal)?;

        let config = self.load_config().await?;
        Ok(DhcpConfigResponse { config })
    }

    async fn toggle(&self, req: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
        auth_context::require_admin()?;

        self.system_config
            .set("dhcp_enabled", if req.enabled { "true" } else { "false" })
            .await
            .map_err(AppError::Internal)?;

        let config = self.load_config().await?;
        Ok(DhcpConfigResponse { config })
    }

    async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError> {
        auth_context::require_admin()?;
        let leases = self
            .dhcp
            .list_active_leases()
            .await
            .map_err(AppError::Internal)?;
        Ok(ListDhcpLeasesResponse { leases })
    }

    async fn revoke_lease(&self, id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError> {
        auth_context::require_admin()?;

        let lease = self
            .dhcp
            .find_lease_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("lease {id} not found")))?;

        if lease.status != DhcpLeaseStatus::Active {
            return Err(AppError::BadRequest("lease is not active".to_owned()));
        }

        self.dhcp
            .update_lease_status(&id.to_string(), "released")
            .await
            .map_err(AppError::Internal)?;

        self.dhcp
            .insert_lease_log(&DhcpLeaseLogRow {
                lease_id: id.to_string(),
                event_type: "released".to_owned(),
                details: Some("admin revoked".to_owned()),
            })
            .await
            .map_err(AppError::Internal)?;

        Ok(RevokeDhcpLeaseResponse {
            message: format!("lease {id} revoked"),
        })
    }

    async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError> {
        auth_context::require_admin()?;
        let reservations = self
            .dhcp
            .list_reservations()
            .await
            .map_err(AppError::Internal)?;
        Ok(ListDhcpReservationsResponse { reservations })
    }

    async fn create_reservation(
        &self,
        req: CreateDhcpReservationRequest,
    ) -> Result<CreateDhcpReservationResponse, AppError> {
        auth_context::require_admin()?;

        // Normalize MAC to lowercase for consistent lookups.
        let mac = req.mac_address.to_lowercase();

        // Validate IP.
        let _: Ipv4Addr = req
            .ip_address
            .parse()
            .map_err(|_| AppError::BadRequest("invalid ip_address".to_owned()))?;

        // Check for duplicate MAC.
        if self
            .dhcp
            .find_reservation_by_mac(&mac)
            .await
            .map_err(AppError::Internal)?
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "reservation for MAC {mac} already exists",
            )));
        }

        // Check for duplicate IP.
        if self
            .dhcp
            .find_reservation_by_ip(&req.ip_address)
            .await
            .map_err(AppError::Internal)?
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "reservation for IP {} already exists",
                req.ip_address
            )));
        }

        let id = Uuid::new_v4();
        let row = DhcpReservationRow {
            id: id.to_string(),
            mac_address: mac.clone(),
            ip_address: req.ip_address.clone(),
            hostname: req.hostname.clone(),
            description: req.description.clone(),
        };

        self.dhcp
            .insert_reservation(&row)
            .await
            .map_err(AppError::Internal)?;

        let reservation = self
            .dhcp
            .find_reservation_by_mac(&mac)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("reservation not found after insert"))
            })?;

        Ok(CreateDhcpReservationResponse {
            reservation,
            message: "reservation created".to_owned(),
        })
    }

    async fn delete_reservation(
        &self,
        id: Uuid,
    ) -> Result<DeleteDhcpReservationResponse, AppError> {
        auth_context::require_admin()?;

        let reservations = self
            .dhcp
            .list_reservations()
            .await
            .map_err(AppError::Internal)?;
        if !reservations.iter().any(|r| r.id == id) {
            return Err(AppError::NotFound(format!("reservation {id} not found")));
        }

        self.dhcp
            .delete_reservation(&id.to_string())
            .await
            .map_err(AppError::Internal)?;

        Ok(DeleteDhcpReservationResponse {
            message: format!("reservation {id} deleted"),
        })
    }

    async fn status(&self) -> Result<DhcpStatusResponse, AppError> {
        auth_context::require_admin()?;

        let config = self.load_config().await?;
        let leases = self
            .dhcp
            .list_active_leases()
            .await
            .map_err(AppError::Internal)?;
        let reservations = self
            .dhcp
            .list_reservations()
            .await
            .map_err(AppError::Internal)?;
        let pool_total = Self::pool_size(config.pool_start, config.pool_end);

        // Count reservations whose IP falls within the pool range.
        let reservations_in_pool = reservations
            .iter()
            .filter(|r| {
                let ip = u32::from(r.ip_address);
                ip >= u32::from(config.pool_start) && ip <= u32::from(config.pool_end)
            })
            .count() as u64;
        let pool_used = leases.len() as u64 + reservations_in_pool;

        Ok(DhcpStatusResponse {
            enabled: config.enabled,
            running: config.enabled, // For now, running == enabled. DhcpRunner will refine this later.
            active_lease_count: leases.len() as u64,
            pool_total,
            pool_used,
        })
    }

    async fn assign_lease(&self, mac: &str, hostname: Option<&str>) -> Result<DhcpLease, AppError> {
        auth_context::require_admin()?;
        let mac = mac.to_lowercase();
        let mac = mac.as_str();

        // If there's already an active lease for this MAC, return it.
        if let Some(existing) = self
            .dhcp
            .find_active_lease_by_mac(mac)
            .await
            .map_err(AppError::Internal)?
        {
            tracing::debug!(mac, ip = %existing.ip_address, "reusing existing active lease");
            return Ok(existing);
        }

        let config = self.load_config().await?;

        // Check for a static reservation first.
        let ip = if let Some(reservation) = self
            .dhcp
            .find_reservation_by_mac(mac)
            .await
            .map_err(AppError::Internal)?
        {
            tracing::info!(mac, ip = %reservation.ip_address, "using static reservation");
            reservation.ip_address
        } else {
            // Find first available IP in pool range.
            self.find_available_ip(&config).await?
        };

        let now = chrono::Utc::now();
        let lease_end = now + chrono::Duration::seconds(i64::from(config.lease_duration_secs));
        let id = Uuid::new_v4();

        let row = DhcpLeaseRow {
            id: id.to_string(),
            mac_address: mac.to_owned(),
            ip_address: ip.to_string(),
            hostname: hostname.map(ToOwned::to_owned),
            lease_start: now.to_rfc3339(),
            lease_end: lease_end.to_rfc3339(),
            status: "active".to_owned(),
            device_id: None,
        };

        self.dhcp
            .insert_lease(&row)
            .await
            .map_err(AppError::Internal)?;

        self.dhcp
            .insert_lease_log(&DhcpLeaseLogRow {
                lease_id: id.to_string(),
                event_type: "assigned".to_owned(),
                details: hostname.map(|h| format!("hostname: {h}")),
            })
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(mac, %ip, lease_id = %id, "DHCP lease assigned");

        // Return the newly created lease.
        self.dhcp
            .find_lease_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("lease not found after insert")))
    }

    async fn renew_lease(&self, mac: &str) -> Result<DhcpLease, AppError> {
        auth_context::require_admin()?;
        let mac = mac.to_lowercase();
        let mac = mac.as_str();

        if let Some(existing) = self
            .dhcp
            .find_active_lease_by_mac(mac)
            .await
            .map_err(AppError::Internal)?
        {
            let config = self.load_config().await?;
            let new_end = chrono::Utc::now()
                + chrono::Duration::seconds(i64::from(config.lease_duration_secs));

            self.dhcp
                .renew_lease(&existing.id.to_string(), &new_end.to_rfc3339())
                .await
                .map_err(AppError::Internal)?;

            self.dhcp
                .insert_lease_log(&DhcpLeaseLogRow {
                    lease_id: existing.id.to_string(),
                    event_type: "renewed".to_owned(),
                    details: Some(format!("new expiry: {new_end}")),
                })
                .await
                .map_err(AppError::Internal)?;

            tracing::info!(mac, lease_id = %existing.id, %new_end, "DHCP lease renewed");

            self.dhcp
                .find_lease_by_id(&existing.id.to_string())
                .await
                .map_err(AppError::Internal)?
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("lease not found after renew")))
        } else {
            // No active lease — assign a new one.
            tracing::info!(mac, "no active lease for renewal, assigning new lease");
            self.assign_lease(mac, None).await
        }
    }

    async fn release_lease(&self, mac: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;
        let mac = mac.to_lowercase();
        let mac = mac.as_str();

        let lease = self
            .dhcp
            .find_active_lease_by_mac(mac)
            .await
            .map_err(AppError::Internal)?;

        if let Some(lease) = lease {
            self.dhcp
                .update_lease_status(&lease.id.to_string(), "released")
                .await
                .map_err(AppError::Internal)?;

            self.dhcp
                .insert_lease_log(&DhcpLeaseLogRow {
                    lease_id: lease.id.to_string(),
                    event_type: "released".to_owned(),
                    details: Some("client DHCPRELEASE".to_owned()),
                })
                .await
                .map_err(AppError::Internal)?;

            tracing::info!(mac, lease_id = %lease.id, "DHCP lease released");
        } else {
            tracing::debug!(mac, "release requested but no active lease found");
        }

        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        auth_context::require_admin()?;

        let count = self
            .dhcp
            .expire_stale_leases()
            .await
            .map_err(AppError::Internal)?;

        if count > 0 {
            tracing::info!(count, "expired stale DHCP leases");
        }

        Ok(count)
    }

    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
        auth_context::require_admin()?;
        self.load_config().await
    }
}
