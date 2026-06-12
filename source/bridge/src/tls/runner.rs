//! Background ACME renewal + multi-host coordination for the bridge's own cert.
//!
//! On every tick each host:
//! 1. **reloads** if the DB cert version overtakes what it serves (so a host that
//!    didn't issue still picks up a freshly issued cert);
//! 2. decides whether issuance is needed (no cert, or inside the renewal window);
//! 3. if so, tries to win the **issuance lease** — only the winner runs the ACME
//!    round-trip, so concurrent hosts never race-burn the Let's Encrypt rate limit.
//!
//! The HTTP-01 token is written to the shared `acme_http_challenge` table, so LE's
//! `:80` validation can land on any host. Cadence is fast while still on the boot
//! placeholder (pick up the first real cert quickly) and 12h + jitter once a real
//! cert is live.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::acme::{self, Http01Solver};
use crate::crypto;
use crate::repository::TlsRepository;
use crate::tls::serving::CertResolver;

/// Steady-state renewal tick.
const RENEWAL_INTERVAL: Duration = Duration::from_hours(12);
/// Extra random delay added to each steady-state tick so hosts don't pile onto
/// the lease together.
const MAX_JITTER: Duration = Duration::from_hours(1);
/// Fast tick while still serving the placeholder (pick up the first cert quickly).
const BOOTSTRAP_INTERVAL: Duration = Duration::from_secs(5);
/// Renew when the leaf expires within this window.
const RENEWAL_WINDOW: chrono::TimeDelta = chrono::TimeDelta::days(30);
/// How long an acquired issuance lease is held before it may be stolen.
const LEASE_TTL: chrono::TimeDelta = chrono::TimeDelta::minutes(5);
/// TTL for a published HTTP-01 challenge token row.
const CHALLENGE_TTL: chrono::TimeDelta = chrono::TimeDelta::minutes(10);

/// The sealed-blob payload: everything needed to serve and renew the cert.
#[derive(Serialize, Deserialize)]
struct CertMaterial {
    account_credentials: Vec<u8>,
    chain_pem: String,
    key_pem: String,
}

/// `true` when `not_after` is within [`RENEWAL_WINDOW`] of `now` (or past).
fn within_renewal_window(not_after: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    not_after - now < RENEWAL_WINDOW
}

/// HTTP-01 solver backed by the shared `acme_http_challenge` table.
struct RepoSolver {
    repo: Arc<dyn TlsRepository>,
}

#[async_trait]
impl Http01Solver for RepoSolver {
    async fn present(&self, token: &str, key_authorization: &str) -> anyhow::Result<()> {
        let expires_at = Utc::now() + CHALLENGE_TTL;
        self.repo
            .put_challenge(token, key_authorization, expires_at)
            .await
    }

    async fn cleanup(&self, token: &str) -> anyhow::Result<()> {
        self.repo.delete_challenge(token).await
    }
}

/// Drives ACME issuance/renewal and keeps [`CertResolver`] in sync with the DB.
pub struct TlsRenewalRunner {
    repo: Arc<dyn TlsRepository>,
    resolver: Arc<CertResolver>,
    solver: RepoSolver,
    fqdn: String,
    directory_url: String,
    encryption_key: [u8; 32],
    /// Per-process lease holder id (so a host can recognise/release its own lease).
    holder: String,
}

impl TlsRenewalRunner {
    #[must_use]
    pub fn new(
        repo: Arc<dyn TlsRepository>,
        resolver: Arc<CertResolver>,
        fqdn: String,
        directory_url: String,
        encryption_key: [u8; 32],
    ) -> Self {
        let solver = RepoSolver {
            repo: Arc::clone(&repo),
        };
        Self {
            repo,
            resolver,
            solver,
            fqdn,
            directory_url,
            encryption_key,
            holder: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Run the renewal loop forever. Spawn as a detached task; each pass is
    /// best-effort and never panics the loop.
    pub async fn run(self) {
        loop {
            if let Err(e) = self.tick().await {
                tracing::warn!(fqdn = %self.fqdn, error = %e, "TLS renewal tick failed");
            }

            let delay = if self.resolver.is_provisioned() {
                RENEWAL_INTERVAL + random_jitter()
            } else {
                BOOTSTRAP_INTERVAL
            };
            tokio::time::sleep(delay).await;
        }
    }

    /// One renewal pass: reload-if-newer, then issue under lease if needed.
    async fn tick(&self) -> anyhow::Result<()> {
        let current = self.repo.load_cert(&self.fqdn).await?;

        // Reload if another host issued a newer cert than we're serving.
        if let Some(row) = &current
            && row.version > self.resolver.served_version()
        {
            let material = self.open_material(&row.sealed_blob, &row.nonce)?;
            self.resolver.install(
                material.chain_pem.as_bytes(),
                material.key_pem.as_bytes(),
                row.version,
            )?;
            tracing::info!(fqdn = %self.fqdn, version = row.version, "reloaded TLS cert from DB");
        }

        let need_issue = match &current {
            None => true,
            Some(row) => within_renewal_window(row.not_after, Utc::now()),
        };
        if !need_issue {
            return Ok(());
        }

        // Only the lease winner issues; others will reload by version next tick.
        let lease_until = Utc::now() + LEASE_TTL;
        if !self
            .repo
            .acquire_lease(&self.fqdn, &self.holder, lease_until)
            .await?
        {
            tracing::debug!(fqdn = %self.fqdn, "another host holds the issuance lease; skipping");
            return Ok(());
        }

        let result = self.issue(current.as_ref()).await;
        // Always release the lease, success or failure.
        if let Err(e) = self.repo.release_lease(&self.fqdn, &self.holder).await {
            tracing::warn!(fqdn = %self.fqdn, error = %e, "failed to release issuance lease");
        }
        result
    }

    /// Run the ACME order, seal + store the result, and hot-swap the live cert.
    async fn issue(&self, current: Option<&crate::repository::SealedCert>) -> anyhow::Result<()> {
        // Reuse the existing ACME account if we have one.
        let account_credentials = match current {
            Some(row) => Some(
                self.open_material(&row.sealed_blob, &row.nonce)?
                    .account_credentials,
            ),
            None => None,
        };

        tracing::info!(fqdn = %self.fqdn, "issuing/renewing TLS certificate via ACME HTTP-01");
        let issued = acme::issue(
            &self.directory_url,
            &self.fqdn,
            account_credentials.as_deref(),
            &self.solver as &dyn Http01Solver,
        )
        .await?;

        let material = CertMaterial {
            account_credentials: issued.account_credentials,
            chain_pem: issued.chain_pem.clone(),
            key_pem: issued.key_pem.clone(),
        };
        let plaintext = serde_json::to_vec(&material)?;
        let (sealed_blob, nonce) = crypto::seal(&self.encryption_key, &plaintext)?;

        let version = self
            .repo
            .store_cert(&self.fqdn, &sealed_blob, &nonce, issued.not_after)
            .await?;

        self.resolver.install(
            issued.chain_pem.as_bytes(),
            issued.key_pem.as_bytes(),
            version,
        )?;
        tracing::info!(
            fqdn = %self.fqdn,
            version,
            not_after = %issued.not_after,
            "installed renewed TLS certificate"
        );
        Ok(())
    }

    /// Decrypt a sealed `bridge_tls` blob into its [`CertMaterial`].
    fn open_material(&self, sealed_blob: &[u8], nonce: &[u8]) -> anyhow::Result<CertMaterial> {
        let plaintext = crypto::open(&self.encryption_key, nonce, sealed_blob)?;
        Ok(serde_json::from_slice(&plaintext)?)
    }
}

/// A uniformly random delay in `[0, MAX_JITTER)`.
fn random_jitter() -> Duration {
    let max = u64::try_from(MAX_JITTER.as_millis()).unwrap_or(u64::MAX);
    Duration::from_millis(rand::random_range(0..max))
}

#[cfg(test)]
mod tests;
