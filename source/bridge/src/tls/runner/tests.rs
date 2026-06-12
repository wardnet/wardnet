use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};

use crate::acme::Http01Solver;
use crate::crypto;
use crate::repository::{SealedCert, TlsRepository};
use crate::tls::{CertResolver, PLACEHOLDER_VERSION, install_crypto_provider};

use super::{CertMaterial, RepoSolver, TlsRenewalRunner, within_renewal_window};

// ── Mock TlsRepository ───────────────────────────────────────────────────────

struct MockTlsRepo {
    load_result: Option<SealedCert>,
    acquire_result: bool,
    challenges: Mutex<HashMap<String, String>>,
}

impl MockTlsRepo {
    fn fresh_cert(version: i64, not_after: DateTime<Utc>) -> Self {
        Self {
            load_result: Some(SealedCert {
                fqdn: "bridge.test".to_owned(),
                sealed_blob: vec![],
                nonce: vec![],
                not_after,
                version,
            }),
            acquire_result: false,
            challenges: Mutex::new(HashMap::new()),
        }
    }

    fn no_cert(acquire_result: bool) -> Self {
        Self {
            load_result: None,
            acquire_result,
            challenges: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl TlsRepository for MockTlsRepo {
    async fn load_cert(&self, _fqdn: &str) -> anyhow::Result<Option<SealedCert>> {
        Ok(self.load_result.clone())
    }

    async fn store_cert(
        &self,
        _fqdn: &str,
        _sealed_blob: &[u8],
        _nonce: &[u8],
        _not_after: DateTime<Utc>,
    ) -> anyhow::Result<i64> {
        Ok(1)
    }

    async fn put_challenge(
        &self,
        token: &str,
        key_authorization: &str,
        _expires_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        self.challenges
            .lock()
            .unwrap()
            .insert(token.to_owned(), key_authorization.to_owned());
        Ok(())
    }

    async fn get_challenge(&self, token: &str) -> anyhow::Result<Option<String>> {
        Ok(self.challenges.lock().unwrap().get(token).cloned())
    }

    async fn delete_challenge(&self, token: &str) -> anyhow::Result<()> {
        self.challenges.lock().unwrap().remove(token);
        Ok(())
    }

    async fn delete_expired_challenges(&self, _now: DateTime<Utc>) -> anyhow::Result<u64> {
        Ok(0)
    }

    async fn acquire_lease(
        &self,
        _fqdn: &str,
        _holder: &str,
        _locked_until: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        Ok(self.acquire_result)
    }

    async fn release_lease(&self, _fqdn: &str, _holder: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn self_signed_pems(fqdn: &str) -> (String, String) {
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let params = rcgen::CertificateParams::new(vec![fqdn.to_owned()]).unwrap();
    let cert = params.self_signed(&key_pair).unwrap();
    (cert.pem(), key_pair.serialize_pem())
}

fn make_runner(
    repo: std::sync::Arc<dyn TlsRepository>,
) -> (TlsRenewalRunner, std::sync::Arc<CertResolver>) {
    install_crypto_provider();
    let resolver = CertResolver::with_placeholder().unwrap();
    let key = [0u8; 32];
    let runner = TlsRenewalRunner::new(
        repo,
        std::sync::Arc::clone(&resolver),
        "bridge.test".to_owned(),
        "https://localhost:14000/dir".to_owned(),
        key,
    );
    (runner, resolver)
}

// ── window tests (kept here — they test a private fn in this module) ─────────

#[test]
fn outside_window_is_not_due() {
    let now = Utc::now();
    assert!(!within_renewal_window(now + TimeDelta::days(60), now));
}

#[test]
fn inside_window_is_due() {
    let now = Utc::now();
    assert!(within_renewal_window(now + TimeDelta::days(20), now));
}

#[test]
fn already_expired_is_due() {
    let now = Utc::now();
    assert!(within_renewal_window(now - TimeDelta::days(1), now));
}

#[test]
fn exactly_at_window_boundary_is_due() {
    let now = Utc::now();
    assert!(within_renewal_window(
        now + TimeDelta::days(30) - TimeDelta::seconds(1),
        now
    ));
}

// ── RepoSolver ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn repo_solver_stores_and_clears_challenge() {
    let repo = std::sync::Arc::new(MockTlsRepo::no_cert(false));
    let solver = RepoSolver {
        repo: std::sync::Arc::clone(&repo) as std::sync::Arc<dyn TlsRepository>,
    };

    solver.present("tok1", "keyauth1").await.unwrap();
    assert_eq!(
        repo.challenges
            .lock()
            .unwrap()
            .get("tok1")
            .map(String::as_str),
        Some("keyauth1"),
        "present must store the key-authorization"
    );

    solver.cleanup("tok1").await.unwrap();
    assert!(
        repo.challenges.lock().unwrap().get("tok1").is_none(),
        "cleanup must remove the token"
    );
}

// ── tick ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tick_skips_issuance_when_cert_is_not_due() {
    // Cert version == PLACEHOLDER_VERSION: no reload. not_after 60 days out: no issuance.
    let not_after = Utc::now() + TimeDelta::days(60);
    let repo = std::sync::Arc::new(MockTlsRepo::fresh_cert(PLACEHOLDER_VERSION, not_after));
    let (runner, resolver) = make_runner(repo);

    runner.tick().await.unwrap();

    assert_eq!(
        resolver.served_version(),
        PLACEHOLDER_VERSION,
        "resolver must still hold the placeholder after a skipped tick"
    );
}

#[tokio::test]
async fn tick_skips_issuance_when_lease_not_won() {
    // No cert in DB → need_issue = true; but acquire_lease returns false.
    let repo = std::sync::Arc::new(MockTlsRepo::no_cert(false));
    let (runner, _resolver) = make_runner(repo);

    runner.tick().await.unwrap();
}

#[tokio::test]
async fn tick_reloads_cert_from_db_when_version_is_newer() {
    install_crypto_provider();

    let key = [0u8; 32];
    let (chain_pem, key_pem) = self_signed_pems("bridge.test");
    let not_after = Utc::now() + TimeDelta::days(60);

    // Seal a real CertMaterial so open_material can decrypt it.
    let material = CertMaterial {
        account_credentials: vec![],
        chain_pem: chain_pem.clone(),
        key_pem: key_pem.clone(),
    };
    let plaintext = serde_json::to_vec(&material).unwrap();
    let (sealed_blob, nonce) = crypto::seal(&key, &plaintext).unwrap();

    let cert_row = SealedCert {
        fqdn: "bridge.test".to_owned(),
        sealed_blob,
        nonce: nonce.to_vec(),
        not_after,
        version: 2,
    };

    let repo = std::sync::Arc::new(MockTlsRepo {
        load_result: Some(cert_row),
        acquire_result: false,
        challenges: Mutex::new(HashMap::new()),
    });
    let resolver = CertResolver::with_placeholder().unwrap();
    let runner = TlsRenewalRunner::new(
        repo,
        std::sync::Arc::clone(&resolver),
        "bridge.test".to_owned(),
        "https://localhost:14000/dir".to_owned(),
        key,
    );

    assert_eq!(resolver.served_version(), PLACEHOLDER_VERSION);
    runner.tick().await.unwrap();

    assert_eq!(
        resolver.served_version(),
        2,
        "tick must hot-swap the resolver to version 2"
    );
    assert!(resolver.is_provisioned());
}
