use async_trait::async_trait;
use wardnet_common::rule_request::{DeviceRuleRequest, RuleRequestKind, RuleRequestStatus};

#[async_trait]
pub trait RuleRequestRepository: Send + Sync {
    /// Insert a new pending request and return the stored row.
    async fn insert(
        &self,
        id: &str,
        device_id: &str,
        kind: RuleRequestKind,
        domain: &str,
        reason: Option<&str>,
        created_at: &str,
    ) -> anyhow::Result<DeviceRuleRequest>;

    /// All requests made by a single device, newest first.
    async fn list_by_device(&self, device_id: &str) -> anyhow::Result<Vec<DeviceRuleRequest>>;

    /// All requests, optionally filtered by status, newest first.
    async fn list_all(
        &self,
        status: Option<RuleRequestStatus>,
    ) -> anyhow::Result<Vec<DeviceRuleRequest>>;

    /// Record an admin decision. Returns the updated row, or `None` if the id
    /// is unknown.
    async fn update_status(
        &self,
        id: &str,
        status: RuleRequestStatus,
        decided_by: &str,
        decided_at: &str,
    ) -> anyhow::Result<Option<DeviceRuleRequest>>;
}
