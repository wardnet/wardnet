//! Thin [instant-acme](https://crates.io/crates/instant-acme) orchestration for
//! DNS-01 certificate issuance, kept behind the [`TlsService`](super::TlsService)
//! boundary so **no live ACME call enters `make check-daemon`** — the order
//! dance here is exercised only by the deferred bridge-live integration test.
//!
//! The leaf keypair and CSR are generated locally with `rcgen`; the private key
//! never leaves the Pi. Challenge TXT records are published through the
//! [`DdnsService`](crate::ddns::DdnsService) and **always** cleared afterwards
//! (success or failure), so a failed issuance can't strand an `_acme-challenge`
//! record.
//!
//! ## Multi-SAN, multi-value challenge
//!
//! The cert is a **per-user wildcard certificate**: one cert with two SANs —
//! the apex `<domain>` and the wildcard `*.<domain>`. Both authorize via DNS-01,
//! and both challenge TXT records live at the *same* `_acme-challenge.<domain>`
//! name with *different* values, so they must be published **simultaneously**.
//!
//! That forces a two-pass order dance: pass 1 collects every authorization's
//! `dns_value` (an `AuthorizationHandle` borrows the order's authorization slice
//! mutably, so only one can be held at a time — we can't ready them inline);
//! pass 2 publishes *all* values in one [`DdnsService::set_acme_challenge`] call
//! and only then marks each challenge ready. Readying a challenge before both
//! values are live would let the CA validate against a half-published name.

use chrono::{DateTime, Utc};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, NewAccount,
    NewOrder, RetryPolicy,
};

use crate::ddns::DdnsService;
use crate::secret_store::SecretStore;

use super::SECRET_ACME_ACCOUNT;

/// A freshly issued certificate: the full chain PEM plus the locally generated
/// leaf private key PEM.
pub struct IssuedCert {
    pub chain_pem: String,
    pub key_pem: String,
}

/// Issue (or renew) a certificate for `domain` via ACME DNS-01.
///
/// Loads or creates the ACME account (credentials persisted in the
/// [`SecretStore`]), runs the order, and **always** clears the challenge TXT
/// before returning.
pub async fn issue(
    ddns: &dyn DdnsService,
    secrets: &dyn SecretStore,
    directory_url: &str,
    domain: &str,
) -> anyhow::Result<IssuedCert> {
    let account = load_or_create_account(secrets, directory_url).await?;
    let result = run_order(&account, ddns, domain).await;

    // Guard: tear down the challenge record regardless of outcome.
    if let Err(e) = ddns.clear_acme_challenge().await {
        tracing::warn!(error = %e, "failed to clear ACME challenge TXT after issuance");
    }

    result
}

/// Restore the ACME account from stored credentials, or create a fresh one and
/// persist its credentials JSON in the secret store.
async fn load_or_create_account(
    secrets: &dyn SecretStore,
    directory_url: &str,
) -> anyhow::Result<Account> {
    if let Some(bytes) = secrets.get(SECRET_ACME_ACCOUNT).await? {
        let credentials: AccountCredentials = serde_json::from_slice(&bytes)?;
        return Ok(Account::builder()?.from_credentials(credentials).await?);
    }

    let (account, credentials) = Account::builder()?
        .create(
            &NewAccount {
                contact: &[],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory_url.to_owned(),
            None,
        )
        .await?;
    secrets
        .put(SECRET_ACME_ACCOUNT, &serde_json::to_vec(&credentials)?)
        .await?;
    tracing::info!("created new ACME account");
    Ok(account)
}

/// Drive a single ACME order for the apex + wildcard SANs to a finalized
/// certificate chain. See the [module docs](self) for why the challenge dance is
/// two-pass.
async fn run_order(
    account: &Account,
    ddns: &dyn DdnsService,
    domain: &str,
) -> anyhow::Result<IssuedCert> {
    let wildcard = format!("*.{domain}");
    let identifiers = [
        Identifier::Dns(domain.to_owned()),
        Identifier::Dns(wildcard.clone()),
    ];
    let mut order = account.new_order(&NewOrder::new(&identifiers)).await?;

    // Pass 1: collect the DNS-01 value for every authorization that isn't
    // already valid. We only gather here — readying a challenge now (before the
    // other SAN's value is published at the same name) would let the CA validate
    // a half-published challenge.
    let mut values: Vec<String> = Vec::new();
    {
        let mut authorizations = order.authorizations();
        while let Some(authz) = authorizations.next().await {
            let mut authz = authz?;
            if authz.status == AuthorizationStatus::Valid {
                continue;
            }
            let challenge = authz.challenge(ChallengeType::Dns01).ok_or_else(|| {
                anyhow::anyhow!("ACME server offered no DNS-01 challenge for {domain}")
            })?;
            values.push(challenge.key_authorization().dns_value());
        }
    }

    // Publish all challenge values at once (both live at `_acme-challenge.<domain>`
    // simultaneously). Skip when every authorization was already valid.
    if !values.is_empty() {
        ddns.set_acme_challenge(&values)
            .await
            .map_err(|e| anyhow::anyhow!("publish ACME challenge: {e}"))?;
    }

    // Pass 2: now that both values are live, mark each pending challenge ready.
    {
        let mut authorizations = order.authorizations();
        while let Some(authz) = authorizations.next().await {
            let mut authz = authz?;
            if authz.status == AuthorizationStatus::Valid {
                continue;
            }
            let mut challenge = authz.challenge(ChallengeType::Dns01).ok_or_else(|| {
                anyhow::anyhow!("ACME server offered no DNS-01 challenge for {domain}")
            })?;
            challenge.set_ready().await?;
        }
    }

    order.poll_ready(&RetryPolicy::default()).await?;

    // CSR + leaf key generated on the Pi. The CSR carries BOTH SANs so the issued
    // cert covers the apex and every per-service host under the wildcard.
    let key_pair = rcgen::KeyPair::generate()?;
    let mut params = rcgen::CertificateParams::new(vec![domain.to_owned(), wildcard])?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    let csr = params.serialize_request(&key_pair)?;
    order.finalize_csr(csr.der().as_ref()).await?;

    let chain_pem = order.poll_certificate(&RetryPolicy::default()).await?;
    let key_pem = key_pair.serialize_pem();

    Ok(IssuedCert { chain_pem, key_pem })
}

/// Parse the leaf certificate's `not_after` from a PEM chain, for renewal
/// scheduling. Reads the first PEM block (the leaf).
pub fn parse_not_after(chain_pem: &[u8]) -> anyhow::Result<DateTime<Utc>> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(chain_pem)
        .map_err(|e| anyhow::anyhow!("failed to parse certificate PEM: {e}"))?;
    let cert = pem
        .parse_x509()
        .map_err(|e| anyhow::anyhow!("failed to parse X.509 certificate: {e}"))?;
    let ts = cert.validity().not_after.timestamp();
    DateTime::from_timestamp(ts, 0)
        .ok_or_else(|| anyhow::anyhow!("certificate not_after timestamp {ts} out of range"))
}
