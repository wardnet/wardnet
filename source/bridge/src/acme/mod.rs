//! Thin [instant-acme](https://crates.io/crates/instant-acme) orchestration for
//! **HTTP-01** certificate issuance of the bridge's *own* FQDN.
//!
//! Unlike the daemon (which is behind home NAT and uses DNS-01 for an apex +
//! wildcard pair), the bridge is publicly reachable on `:80`, so it proves
//! control of its single hostname by serving the key authorization at
//! `/.well-known/acme-challenge/{token}`. The token is handed to an
//! [`Http01Solver`] — in production backed by the shared `acme_http_challenge`
//! table so any host's `:8080` responder can answer Let's Encrypt's validation.
//!
//! Persistence is kept **out** of this module: the caller passes in the existing
//! ACME account credentials (if any) and gets the credentials back out in
//! [`IssuedCert`], to seal and store however it likes. The leaf key is generated
//! locally with `rcgen` and never leaves this process unencrypted.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use instant_acme::{
    Account, AccountBuilder, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier,
    NewAccount, NewOrder, RetryPolicy,
};

/// Presents and tears down an ACME HTTP-01 challenge response.
#[async_trait]
pub trait Http01Solver: Send + Sync {
    /// Make `key_authorization` available at
    /// `/.well-known/acme-challenge/{token}` for validation.
    async fn present(&self, token: &str, key_authorization: &str) -> anyhow::Result<()>;

    /// Remove the published challenge response for `token` (idempotent).
    async fn cleanup(&self, token: &str) -> anyhow::Result<()>;
}

/// A freshly issued certificate plus the account credentials to persist.
pub struct IssuedCert {
    pub chain_pem: String,
    pub key_pem: String,
    /// Account credentials JSON — the input echoed back, or freshly created.
    pub account_credentials: Vec<u8>,
    /// Leaf certificate expiry, parsed from the issued chain.
    pub not_after: DateTime<Utc>,
}

/// Issue (or renew) a certificate for `fqdn` via ACME HTTP-01.
///
/// Loads the ACME account from `account_credentials` (or creates a new one),
/// runs the order through the [`Http01Solver`], and parses the leaf expiry.
///
/// # Errors
/// Propagates any ACME, `rcgen`, or solver failure.
pub async fn issue(
    directory_url: &str,
    fqdn: &str,
    account_credentials: Option<&[u8]>,
    solver: &dyn Http01Solver,
) -> anyhow::Result<IssuedCert> {
    let (account, account_credentials) =
        load_or_create_account(directory_url, account_credentials).await?;

    let (chain_pem, key_pem) = run_order(&account, fqdn, solver).await?;
    let not_after = parse_not_after(chain_pem.as_bytes())?;

    Ok(IssuedCert {
        chain_pem,
        key_pem,
        account_credentials,
        not_after,
    })
}

/// Returns an [`AccountBuilder`] that trusts the pebble WFE CA when the
/// `BRIDGE_TEST_PEBBLE_CA` env var is set (integration-test harness only).
fn make_account_builder() -> anyhow::Result<AccountBuilder> {
    #[cfg(test)]
    if let Ok(path) = std::env::var("BRIDGE_TEST_PEBBLE_CA") {
        return Ok(Account::builder_with_root(path)?);
    }
    Ok(Account::builder()?)
}

/// Restore the ACME account from `credentials`, or create a fresh one and return
/// its serialized credentials JSON for the caller to persist.
async fn load_or_create_account(
    directory_url: &str,
    credentials: Option<&[u8]>,
) -> anyhow::Result<(Account, Vec<u8>)> {
    if let Some(bytes) = credentials {
        let creds: AccountCredentials = serde_json::from_slice(bytes)?;
        let account = make_account_builder()?.from_credentials(creds).await?;
        return Ok((account, bytes.to_vec()));
    }

    let (account, creds) = make_account_builder()?
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
    tracing::info!("created new ACME account");
    Ok((account, serde_json::to_vec(&creds)?))
}

/// Drive a single HTTP-01 order for `fqdn` to a finalized certificate chain. The
/// challenge token is **always** cleaned up afterwards (success or failure).
async fn run_order(
    account: &Account,
    fqdn: &str,
    solver: &dyn Http01Solver,
) -> anyhow::Result<(String, String)> {
    let identifiers = [Identifier::Dns(fqdn.to_owned())];
    let mut order = account.new_order(&NewOrder::new(&identifiers)).await?;

    let token = present_challenge(&mut order, solver).await?;

    let result = finalize_order(&mut order, fqdn).await;

    if let Some(token) = &token
        && let Err(e) = solver.cleanup(token).await
    {
        tracing::warn!(error = %e, "failed to clean up HTTP-01 challenge token after issuance");
    }

    result
}

/// Present the (single) HTTP-01 challenge and mark it ready. Returns the token,
/// or `None` if the authorization was already valid (a reused order).
async fn present_challenge(
    order: &mut instant_acme::Order,
    solver: &dyn Http01Solver,
) -> anyhow::Result<Option<String>> {
    let mut authorizations = order.authorizations();
    while let Some(authz) = authorizations.next().await {
        let mut authz = authz?;
        if authz.status == AuthorizationStatus::Valid {
            continue;
        }
        let mut challenge = authz
            .challenge(ChallengeType::Http01)
            .ok_or_else(|| anyhow::anyhow!("ACME server offered no HTTP-01 challenge"))?;
        let token = challenge.token.clone();
        let key_authorization = challenge.key_authorization().as_str().to_owned();

        solver.present(&token, &key_authorization).await?;
        challenge.set_ready().await?;
        return Ok(Some(token));
    }
    Ok(None)
}

/// Poll for validation, generate the leaf key + CSR locally, finalize, and fetch
/// the issued chain.
async fn finalize_order(
    order: &mut instant_acme::Order,
    fqdn: &str,
) -> anyhow::Result<(String, String)> {
    order.poll_ready(&RetryPolicy::default()).await?;

    let key_pair = rcgen::KeyPair::generate()?;
    let mut params = rcgen::CertificateParams::new(vec![fqdn.to_owned()])?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    let csr = params.serialize_request(&key_pair)?;
    order.finalize_csr(csr.der().as_ref()).await?;

    let chain_pem = order.poll_certificate(&RetryPolicy::default()).await?;
    let key_pem = key_pair.serialize_pem();
    Ok((chain_pem, key_pem))
}

/// Parse the leaf certificate's `not_after` from a PEM chain (reads the first PEM
/// block, the leaf), for renewal scheduling.
///
/// # Errors
/// Returns an error if the PEM/X.509 cannot be parsed or the timestamp is out of
/// range.
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

#[cfg(test)]
mod tests;
