use async_trait::async_trait;
use wardnet_common::zone_exception::ZoneException;

/// Data access for cross-zone exceptions (epic #244, issue #737).
///
/// Exceptions are admin-granted allowances for one endpoint to reach another
/// across an otherwise-isolated zone boundary. All business logic (endpoint
/// existence, port validation) lives in
/// [`ZoneExceptionService`](crate::service::ZoneExceptionService); this trait is
/// pure persistence.
#[async_trait]
pub trait ZoneExceptionRepository: Send + Sync {
    /// Return all exceptions.
    async fn find_all(&self) -> anyhow::Result<Vec<ZoneException>>;

    /// Find an exception by its primary key.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<ZoneException>>;

    /// Insert a new exception.
    async fn insert(&self, exception: &ZoneException) -> anyhow::Result<()>;

    /// Update an existing exception by id (all mutable columns).
    async fn update(&self, exception: &ZoneException) -> anyhow::Result<()>;

    /// Delete an exception by id.
    async fn delete(&self, id: &str) -> anyhow::Result<()>;
}
