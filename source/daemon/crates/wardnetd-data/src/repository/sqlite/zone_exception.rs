use async_trait::async_trait;
use sqlx::SqlitePool;
use wardnet_common::zone_exception::{
    ExceptionEndpoint, ExceptionEndpointKind, PortSpec, ServiceSet, ServiceSpec, ZoneException,
};

use super::super::ZoneExceptionRepository;
use crate::db::DbPools;

/// SQLite-backed implementation of [`ZoneExceptionRepository`].
pub struct SqliteZoneExceptionRepository {
    pools: DbPools,
}

impl SqliteZoneExceptionRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    /// Create a new repository with split reader/writer pools.
    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

/// Raw row from the `zone_exceptions` table used for internal mapping.
#[derive(sqlx::FromRow)]
struct ExceptionRow {
    id: String,
    from_kind: String,
    from_id: String,
    to_kind: String,
    to_id: String,
    service_preset: Option<String>,
    ports_json: Option<String>,
    bidirectional: i64,
    created_at: String,
    updated_at: String,
}

impl ExceptionRow {
    fn into_exception(self) -> anyhow::Result<ZoneException> {
        Ok(ZoneException {
            id: self.id.parse()?,
            from: endpoint(&self.from_kind, &self.from_id)?,
            to: endpoint(&self.to_kind, &self.to_id)?,
            service: service_spec(self.service_preset.as_deref(), self.ports_json.as_deref())?,
            bidirectional: self.bidirectional != 0,
            created_at: self.created_at.parse()?,
            updated_at: self.updated_at.parse()?,
        })
    }
}

/// Reconstruct an [`ExceptionEndpoint`] from its `kind` string + id.
fn endpoint(kind: &str, id: &str) -> anyhow::Result<ExceptionEndpoint> {
    let kind: ExceptionEndpointKind = serde_json::from_str(&format!("\"{kind}\""))?;
    Ok(ExceptionEndpoint {
        kind,
        id: id.parse()?,
    })
}

/// Reconstruct a [`ServiceSpec`] from its persisted columns: exactly one of
/// `service_preset` / `ports_json` is non-NULL.
fn service_spec(preset: Option<&str>, ports_json: Option<&str>) -> anyhow::Result<ServiceSpec> {
    match (preset, ports_json) {
        (Some(preset), _) => {
            let set: ServiceSet = serde_json::from_str(&format!("\"{preset}\""))?;
            Ok(ServiceSpec::Preset { set })
        }
        (None, Some(json)) => {
            let ports: Vec<PortSpec> = serde_json::from_str(json)?;
            Ok(ServiceSpec::Ports { ports })
        }
        (None, None) => Err(anyhow::anyhow!(
            "zone_exceptions row has neither service_preset nor ports_json"
        )),
    }
}

const SELECT_COLS: &str = "id, from_kind, from_id, to_kind, to_id, service_preset, ports_json, \
    bidirectional, created_at, updated_at";

/// Serialize a scalar enum to its `snake_case` wire string (strip the JSON quotes).
fn enum_str<T: serde::Serialize>(value: &T) -> anyhow::Result<String> {
    Ok(serde_json::to_string(value)?.trim_matches('"').to_owned())
}

/// Split a [`ServiceSpec`] into the `(service_preset, ports_json)` column pair.
fn service_columns(service: &ServiceSpec) -> anyhow::Result<(Option<String>, Option<String>)> {
    match service {
        ServiceSpec::Preset { set } => Ok((Some(enum_str(set)?), None)),
        ServiceSpec::Ports { ports } => Ok((None, Some(serde_json::to_string(ports)?))),
    }
}

#[async_trait]
impl ZoneExceptionRepository for SqliteZoneExceptionRepository {
    async fn find_all(&self) -> anyhow::Result<Vec<ZoneException>> {
        let query = format!("SELECT {SELECT_COLS} FROM zone_exceptions ORDER BY created_at");
        let rows = sqlx::query_as::<_, ExceptionRow>(sqlx::AssertSqlSafe(query))
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter().map(ExceptionRow::into_exception).collect()
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<ZoneException>> {
        let query = format!("SELECT {SELECT_COLS} FROM zone_exceptions WHERE id = ?");
        let row = sqlx::query_as::<_, ExceptionRow>(sqlx::AssertSqlSafe(query))
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(ExceptionRow::into_exception).transpose()
    }

    async fn insert(&self, exception: &ZoneException) -> anyhow::Result<()> {
        let (service_preset, ports_json) = service_columns(&exception.service)?;
        sqlx::query(
            "INSERT INTO zone_exceptions \
             (id, from_kind, from_id, to_kind, to_id, service_preset, ports_json, \
              bidirectional, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(exception.id.to_string())
        .bind(enum_str(&exception.from.kind)?)
        .bind(exception.from.id.to_string())
        .bind(enum_str(&exception.to.kind)?)
        .bind(exception.to.id.to_string())
        .bind(service_preset)
        .bind(ports_json)
        .bind(i64::from(exception.bidirectional))
        .bind(exception.created_at.to_rfc3339())
        .bind(exception.updated_at.to_rfc3339())
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn update(&self, exception: &ZoneException) -> anyhow::Result<()> {
        let (service_preset, ports_json) = service_columns(&exception.service)?;
        sqlx::query(
            "UPDATE zone_exceptions SET \
                from_kind = ?, from_id = ?, to_kind = ?, to_id = ?, \
                service_preset = ?, ports_json = ?, bidirectional = ?, updated_at = ? \
             WHERE id = ?",
        )
        .bind(enum_str(&exception.from.kind)?)
        .bind(exception.from.id.to_string())
        .bind(enum_str(&exception.to.kind)?)
        .bind(exception.to.id.to_string())
        .bind(service_preset)
        .bind(ports_json)
        .bind(i64::from(exception.bidirectional))
        .bind(exception.updated_at.to_rfc3339())
        .bind(exception.id.to_string())
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM zone_exceptions WHERE id = ?")
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }
}
