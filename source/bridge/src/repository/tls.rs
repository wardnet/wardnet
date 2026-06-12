//! Data access for the bridge's own TLS material and ACME coordination state:
//! the sealed certificate row (`bridge_tls`), live HTTP-01 challenge tokens
//! (`acme_http_challenge`), and the issuance lease (`bridge_tls_lease`).
//!
//! See [`migrations/20260620000000_bridge_tls.sql`] for the schema and the
//! multi-host rationale.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::db::DbPools;

/// A sealed certificate row for one FQDN.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SealedCert {
    pub fqdn: String,
    /// AES-256-GCM ciphertext of `{account_credentials, chain_pem, key_pem}`.
    pub sealed_blob: Vec<u8>,
    /// 96-bit GCM nonce for `sealed_blob`.
    pub nonce: Vec<u8>,
    /// Leaf certificate expiry (plaintext, for renewal scheduling).
    pub not_after: DateTime<Utc>,
    /// Bumped on every successful (re)issue; drives cross-host reload.
    pub version: i64,
}

const LOAD_CERT: &str =
    "SELECT fqdn, sealed_blob, nonce, not_after, version FROM bridge_tls WHERE fqdn = $1";

const STORE_CERT: &str = "INSERT INTO bridge_tls (fqdn, sealed_blob, nonce, not_after) \
     VALUES ($1, $2, $3, $4) \
     ON CONFLICT (fqdn) DO UPDATE SET \
        sealed_blob = EXCLUDED.sealed_blob, \
        nonce = EXCLUDED.nonce, \
        not_after = EXCLUDED.not_after, \
        version = bridge_tls.version + 1, \
        updated_at = now() \
     RETURNING version";

const PUT_CHALLENGE: &str = "INSERT INTO acme_http_challenge (token, key_authorization, expires_at) \
     VALUES ($1, $2, $3) \
     ON CONFLICT (token) DO UPDATE SET \
        key_authorization = EXCLUDED.key_authorization, \
        expires_at = EXCLUDED.expires_at";

const GET_CHALLENGE: &str =
    "SELECT key_authorization FROM acme_http_challenge WHERE token = $1 AND expires_at > now()";

const DELETE_CHALLENGE: &str = "DELETE FROM acme_http_challenge WHERE token = $1";

const DELETE_EXPIRED_CHALLENGES: &str = "DELETE FROM acme_http_challenge WHERE expires_at <= $1";

const ACQUIRE_LEASE: &str = "INSERT INTO bridge_tls_lease (fqdn, holder, locked_until) \
     VALUES ($1, $2, $3) \
     ON CONFLICT (fqdn) DO UPDATE SET holder = $2, locked_until = $3 \
     WHERE bridge_tls_lease.locked_until IS NULL OR bridge_tls_lease.locked_until < now()";

const RELEASE_LEASE: &str = "UPDATE bridge_tls_lease SET holder = NULL, locked_until = NULL WHERE fqdn = $1 AND holder = $2";

/// Data access for the bridge's own TLS material.
#[async_trait]
pub trait TlsRepository: Send + Sync {
    /// Load the sealed certificate row for `fqdn`, if one has been issued.
    async fn load_cert(&self, fqdn: &str) -> anyhow::Result<Option<SealedCert>>;

    /// Upsert the sealed certificate for `fqdn`, bumping `version`. Returns the
    /// new version, which other hosts compare against to trigger a reload.
    async fn store_cert(
        &self,
        fqdn: &str,
        sealed_blob: &[u8],
        nonce: &[u8],
        not_after: DateTime<Utc>,
    ) -> anyhow::Result<i64>;

    /// Publish a live HTTP-01 challenge token → key-authorization (any host's
    /// `:8080` serves it). Upsert so a retried order overwrites cleanly.
    async fn put_challenge(
        &self,
        token: &str,
        key_authorization: &str,
        expires_at: DateTime<Utc>,
    ) -> anyhow::Result<()>;

    /// Look up the key-authorization for a **non-expired** challenge `token`.
    async fn get_challenge(&self, token: &str) -> anyhow::Result<Option<String>>;

    /// Delete a challenge token (post-issuance cleanup; idempotent).
    async fn delete_challenge(&self, token: &str) -> anyhow::Result<()>;

    /// Reap every challenge that expired at or before `now`. Returns the count.
    async fn delete_expired_challenges(&self, now: DateTime<Utc>) -> anyhow::Result<u64>;

    /// Try to acquire the issuance lease for `fqdn` until `locked_until`. Returns
    /// `true` if this host now holds the lease, `false` if another host holds a
    /// still-valid one.
    async fn acquire_lease(
        &self,
        fqdn: &str,
        holder: &str,
        locked_until: DateTime<Utc>,
    ) -> anyhow::Result<bool>;

    /// Release a lease this host holds (no-op if `holder` no longer matches).
    async fn release_lease(&self, fqdn: &str, holder: &str) -> anyhow::Result<()>;
}

/// PostgreSQL-backed [`TlsRepository`].
pub struct PgTlsRepository {
    pools: DbPools,
}

impl PgTlsRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pools: DbPools::single(pool),
        }
    }

    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl TlsRepository for PgTlsRepository {
    async fn load_cert(&self, fqdn: &str) -> anyhow::Result<Option<SealedCert>> {
        Ok(sqlx::query_as::<_, SealedCert>(LOAD_CERT)
            .bind(fqdn)
            .fetch_optional(&self.pools.read)
            .await?)
    }

    async fn store_cert(
        &self,
        fqdn: &str,
        sealed_blob: &[u8],
        nonce: &[u8],
        not_after: DateTime<Utc>,
    ) -> anyhow::Result<i64> {
        let version: i64 = sqlx::query_scalar(STORE_CERT)
            .bind(fqdn)
            .bind(sealed_blob)
            .bind(nonce)
            .bind(not_after)
            .fetch_one(&self.pools.write)
            .await?;
        Ok(version)
    }

    async fn put_challenge(
        &self,
        token: &str,
        key_authorization: &str,
        expires_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(PUT_CHALLENGE)
            .bind(token)
            .bind(key_authorization)
            .bind(expires_at)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn get_challenge(&self, token: &str) -> anyhow::Result<Option<String>> {
        Ok(sqlx::query_scalar(GET_CHALLENGE)
            .bind(token)
            .fetch_optional(&self.pools.read)
            .await?)
    }

    async fn delete_challenge(&self, token: &str) -> anyhow::Result<()> {
        sqlx::query(DELETE_CHALLENGE)
            .bind(token)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn delete_expired_challenges(&self, now: DateTime<Utc>) -> anyhow::Result<u64> {
        let rows = sqlx::query(DELETE_EXPIRED_CHALLENGES)
            .bind(now)
            .execute(&self.pools.write)
            .await?
            .rows_affected();
        Ok(rows)
    }

    async fn acquire_lease(
        &self,
        fqdn: &str,
        holder: &str,
        locked_until: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        let rows = sqlx::query(ACQUIRE_LEASE)
            .bind(fqdn)
            .bind(holder)
            .bind(locked_until)
            .execute(&self.pools.write)
            .await?
            .rows_affected();
        Ok(rows > 0)
    }

    async fn release_lease(&self, fqdn: &str, holder: &str) -> anyhow::Result<()> {
        sqlx::query(RELEASE_LEASE)
            .bind(fqdn)
            .bind(holder)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }
}
