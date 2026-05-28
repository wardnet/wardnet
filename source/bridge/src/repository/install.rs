use async_trait::async_trait;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use crate::db::DbPools;

/// A registered wardnet installation.
///
/// One row per Pi that has completed the DDNS setup step. The `name` field
/// is the user-chosen subdomain slug (e.g. `happy-einstein`); the `id` is a
/// server-assigned `UUIDv4` used in all subsequent API paths.
#[derive(Debug, Clone)]
pub struct Install {
    pub id: String,
    /// Subdomain slug — validated as `[a-z0-9-]`, 3–32 chars.
    pub name: String,
    /// Base64-encoded raw Ed25519 verifying-key bytes (32 bytes).
    pub public_key: String,
    /// Raw Ed25519 verifying-key bytes, decoded once on row load.
    ///
    /// Avoids repeated base64 decoding + allocation on every authenticated
    /// request. Kept in sync with `public_key` — both are set from the same
    /// database column via [`InstallRow::into_install`].
    pub pub_key_bytes: [u8; 32],
    /// Hex SHA-256 of the bearer token — the raw token is never stored.
    pub token_hash: String,
    /// Last known public IPv4 address; `None` until the first PUT /ip.
    pub ip: Option<String>,
    /// Cloudflare DNS record ID for the A record; `None` until created.
    pub cf_a_record_id: Option<String>,
    /// Cloudflare DNS record ID for the active ACME TXT record; `None` when no challenge is live.
    pub cf_acme_record_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Raw `SQLite` row — used for `sqlx::query_as` mapping.
#[derive(sqlx::FromRow)]
struct InstallRow {
    id: String,
    name: String,
    public_key: String,
    token_hash: String,
    ip: Option<String>,
    cf_a_record_id: Option<String>,
    cf_acme_record_id: Option<String>,
    created_at: String,
    updated_at: String,
}

impl InstallRow {
    fn into_install(self) -> anyhow::Result<Install> {
        let pk_bytes = base64::engine::general_purpose::STANDARD
            .decode(&self.public_key)
            .map_err(|e| anyhow::anyhow!("base64-decode public_key for install {}: {e}", self.id))?;
        let pub_key_bytes: [u8; 32] = pk_bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("Ed25519 public key for install {} must be 32 bytes", self.id))?;

        Ok(Install {
            id: self.id,
            name: self.name,
            public_key: self.public_key,
            pub_key_bytes,
            token_hash: self.token_hash,
            ip: self.ip,
            cf_a_record_id: self.cf_a_record_id,
            cf_acme_record_id: self.cf_acme_record_id,
            created_at: self.created_at.parse()?,
            updated_at: self.updated_at.parse()?,
        })
    }
}

// Constant query strings avoid a heap allocation per call when
// compared to `format!("SELECT {SELECT_COLS} FROM installs WHERE ...")`.
const FIND_BY_ID: &str =
    "SELECT id, name, public_key, token_hash, ip, cf_a_record_id, cf_acme_record_id, \
     created_at, updated_at FROM installs WHERE id = ?";

const FIND_BY_NAME: &str =
    "SELECT id, name, public_key, token_hash, ip, cf_a_record_id, cf_acme_record_id, \
     created_at, updated_at FROM installs WHERE name = ?";

const FIND_BY_TOKEN_HASH: &str =
    "SELECT id, name, public_key, token_hash, ip, cf_a_record_id, cf_acme_record_id, \
     created_at, updated_at FROM installs WHERE token_hash = ?";

/// Data access for the `installs` and `registration_log` tables.
///
/// All business logic (rate-limit checks, name validation) lives in the
/// API handlers and service layer — this trait is purely I/O.
#[async_trait]
pub trait InstallRepository: Send + Sync {
    /// Find an install by its server-assigned UUID.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Install>>;

    /// Find an install by its subdomain name.
    async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<Install>>;

    /// Find an install by the hex SHA-256 of its bearer token.
    async fn find_by_token_hash(&self, token_hash: &str) -> anyhow::Result<Option<Install>>;

    /// Persist a new installation record.
    async fn insert(&self, install: &Install) -> anyhow::Result<()>;

    /// Update the public IP address and Cloudflare A-record ID after a successful DNS upsert.
    async fn update_ip(
        &self,
        id: &str,
        ip: &str,
        cf_a_record_id: &str,
        updated_at: &str,
    ) -> anyhow::Result<()>;

    /// Update (or clear) the Cloudflare ACME TXT-record ID.
    ///
    /// Pass `None` to clear the field after the TXT record has been deleted.
    async fn update_acme_record(
        &self,
        id: &str,
        cf_acme_record_id: Option<&str>,
        updated_at: &str,
    ) -> anyhow::Result<()>;

    /// Delete an installation record.
    async fn delete(&self, id: &str) -> anyhow::Result<()>;

    /// Count how many registrations have been attempted from `remote_ip`
    /// since `since` (ISO 8601 UTC).
    async fn count_registrations_from_ip(
        &self,
        remote_ip: &str,
        since: &str,
    ) -> anyhow::Result<i64>;

    /// Append a row to `registration_log` for rate-limit tracking.
    async fn log_registration(&self, remote_ip: &str, created_at: &str) -> anyhow::Result<()>;
}

/// SQLite-backed [`InstallRepository`].
pub struct SqliteInstallRepository {
    pools: DbPools,
}

impl SqliteInstallRepository {
    /// Create a repository backed by a single pool (tests / in-memory).
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pools: DbPools::single(pool) }
    }

    /// Create a repository with split reader / writer pools.
    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl InstallRepository for SqliteInstallRepository {
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Install>> {
        sqlx::query_as::<_, InstallRow>(FIND_BY_ID)
            .bind(id)
            .fetch_optional(&self.pools.read)
            .await?
            .map(InstallRow::into_install)
            .transpose()
    }

    async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<Install>> {
        sqlx::query_as::<_, InstallRow>(FIND_BY_NAME)
            .bind(name)
            .fetch_optional(&self.pools.read)
            .await?
            .map(InstallRow::into_install)
            .transpose()
    }

    async fn find_by_token_hash(&self, token_hash: &str) -> anyhow::Result<Option<Install>> {
        sqlx::query_as::<_, InstallRow>(FIND_BY_TOKEN_HASH)
            .bind(token_hash)
            .fetch_optional(&self.pools.read)
            .await?
            .map(InstallRow::into_install)
            .transpose()
    }

    async fn insert(&self, install: &Install) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO installs
             (id, name, public_key, token_hash, ip, cf_a_record_id, cf_acme_record_id,
              created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&install.id)
        .bind(&install.name)
        .bind(&install.public_key)
        .bind(&install.token_hash)
        .bind(&install.ip)
        .bind(&install.cf_a_record_id)
        .bind(&install.cf_acme_record_id)
        .bind(install.created_at.to_rfc3339())
        .bind(install.updated_at.to_rfc3339())
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn update_ip(
        &self,
        id: &str,
        ip: &str,
        cf_a_record_id: &str,
        updated_at: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE installs SET ip = ?, cf_a_record_id = ?, updated_at = ? WHERE id = ?",
        )
        .bind(ip)
        .bind(cf_a_record_id)
        .bind(updated_at)
        .bind(id)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn update_acme_record(
        &self,
        id: &str,
        cf_acme_record_id: Option<&str>,
        updated_at: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE installs SET cf_acme_record_id = ?, updated_at = ? WHERE id = ?",
        )
        .bind(cf_acme_record_id)
        .bind(updated_at)
        .bind(id)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM installs WHERE id = ?")
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn count_registrations_from_ip(
        &self,
        remote_ip: &str,
        since: &str,
    ) -> anyhow::Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM registration_log WHERE remote_ip = ? AND created_at > ?",
        )
        .bind(remote_ip)
        .bind(since)
        .fetch_one(&self.pools.read)
        .await?;
        Ok(count)
    }

    async fn log_registration(&self, remote_ip: &str, created_at: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO registration_log (remote_ip, created_at) VALUES (?, ?)",
        )
        .bind(remote_ip)
        .bind(created_at)
        .execute(&self.pools.write)
        .await?;
        Ok(())
    }
}
