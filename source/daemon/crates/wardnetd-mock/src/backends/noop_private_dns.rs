//! In-memory Private DNS service for the mock (issues #912/#914).
//!
//! The real [`PrivateDnsServiceImpl`](wardnetd_services::private_dns) is wired
//! to the live DDNS/TLS services and secret store; in the mock those are the
//! offline stand-ins (`noop_remote_access`), so — exactly as the mock swaps in
//! `MockDdnsService`/`MockTlsService` — it swaps in this stateful fake. It backs
//! the full admin flow (enable, grant, revoke) against in-memory state and signs
//! a real `.mobileconfig` with a one-off self-signed cert so the profile
//! download works offline.

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use rand::RngExt;
use uuid::Uuid;
use wardnetd_services::error::AppError;
use wardnetd_services::private_dns::profile;
use wardnetd_services::private_dns::{PrivateDnsGrant, PrivateDnsService, PrivateDnsStatus};

/// The demo wardnet domain grants live under — matches the mock's demo DDNS
/// host (`noop_remote_access::configure_demo`) so the UI reads coherently.
const DEMO_DOMAIN: &str = "home1.demo.wardnet.services";

/// RFC 4648 base-32 alphabet, lowercased — mirrors the real token minting.
const BASE32_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

struct Inner {
    enabled: bool,
    grants: Vec<PrivateDnsGrant>,
}

/// Stateful in-memory [`PrivateDnsService`]. Provisioned and enabled out of the
/// box (the mock always stands in for a fully subscribed box).
pub struct MockPrivateDnsService {
    inner: Mutex<Inner>,
    /// A one-off self-signed P-256 cert/key, standing in for the box's live
    /// Let's Encrypt pair so `device_profile` can produce a signed profile.
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
}

impl Default for MockPrivateDnsService {
    fn default() -> Self {
        let key_pair = rcgen::KeyPair::generate().expect("mock: generate profile keypair");
        let cert = rcgen::CertificateParams::new(vec![format!("*.{DEMO_DOMAIN}")])
            .expect("mock: cert params")
            .self_signed(&key_pair)
            .expect("mock: self-sign profile cert");
        Self {
            inner: Mutex::new(Inner {
                enabled: true,
                grants: Vec::new(),
            }),
            cert_pem: cert.pem().into_bytes(),
            key_pem: key_pair.serialize_pem().into_bytes(),
        }
    }
}

impl MockPrivateDnsService {
    fn mint_token() -> String {
        let mut rng = rand::rng();
        (0..16)
            .map(|_| char::from(BASE32_ALPHABET[rng.random_range(0..32)]))
            .collect()
    }

    fn hostname(token: &str) -> String {
        format!("{token}.{DEMO_DOMAIN}")
    }
}

#[async_trait]
impl PrivateDnsService for MockPrivateDnsService {
    async fn status(&self) -> Result<PrivateDnsStatus, AppError> {
        Ok(PrivateDnsStatus {
            enabled: self.inner.lock().unwrap().enabled,
            domain: Some(DEMO_DOMAIN.to_owned()),
        })
    }

    async fn is_enabled(&self) -> Result<bool, AppError> {
        Ok(self.inner.lock().unwrap().enabled)
    }

    async fn set_enabled(&self, enabled: bool) -> Result<PrivateDnsStatus, AppError> {
        self.inner.lock().unwrap().enabled = enabled;
        Ok(PrivateDnsStatus {
            enabled,
            domain: Some(DEMO_DOMAIN.to_owned()),
        })
    }

    async fn grant_device(&self, device_id: Uuid) -> Result<PrivateDnsGrant, AppError> {
        let mut inner = self.inner.lock().unwrap();
        if !inner.enabled {
            return Err(AppError::Conflict("Private DNS is disabled".to_owned()));
        }
        if inner.grants.iter().any(|g| g.device_id == device_id) {
            return Err(AppError::Conflict(
                "device already has a Private DNS grant".to_owned(),
            ));
        }
        let grant = PrivateDnsGrant {
            id: Uuid::new_v4(),
            device_id,
            token: Self::mint_token(),
            created_at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        };
        inner.grants.push(grant.clone());
        Ok(grant)
    }

    async fn revoke_grant(&self, grant_id: Uuid) -> Result<(), AppError> {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.grants.len();
        inner.grants.retain(|g| g.id != grant_id);
        if inner.grants.len() == before {
            return Err(AppError::NotFound(format!("grant {grant_id} not found")));
        }
        Ok(())
    }

    async fn list_grants(&self) -> Result<Vec<PrivateDnsGrant>, AppError> {
        Ok(self.inner.lock().unwrap().grants.clone())
    }

    async fn device_grant(&self, device_id: Uuid) -> Result<Option<PrivateDnsGrant>, AppError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .grants
            .iter()
            .find(|g| g.device_id == device_id)
            .cloned())
    }

    async fn device_profile(&self, device_id: Uuid) -> Result<Option<Vec<u8>>, AppError> {
        let (enabled, grant) = {
            let inner = self.inner.lock().unwrap();
            (
                inner.enabled,
                inner
                    .grants
                    .iter()
                    .find(|g| g.device_id == device_id)
                    .cloned(),
            )
        };
        if !enabled {
            return Ok(None);
        }
        let Some(grant) = grant else {
            return Ok(None);
        };
        let signed = profile::build_signed_profile(
            &Self::hostname(&grant.token),
            &self.cert_pem,
            &self.key_pem,
        )
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
        Ok(Some(signed))
    }

    async fn resolve_token(&self, token: &str) -> Result<Option<PrivateDnsGrant>, AppError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .grants
            .iter()
            .find(|g| g.token == token)
            .cloned())
    }

    async fn reconcile(&self) -> Result<(), AppError> {
        Ok(())
    }
}
