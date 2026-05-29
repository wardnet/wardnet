use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use wardnet_bridge::repository::{
    ChallengeRepository, Install, InstallRepository, RegistrationChallenge,
};

// ── Mock install repository ──────────────────────────────────────────────────

pub struct MockInstallRepository {
    installs: Mutex<HashMap<String, Install>>,
    log: Mutex<Vec<(String, DateTime<Utc>)>>,
}

impl MockInstallRepository {
    pub fn new() -> Self {
        Self {
            installs: Mutex::new(HashMap::new()),
            log: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl InstallRepository for MockInstallRepository {
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Install>> {
        Ok(self.installs.lock().unwrap().get(id).cloned())
    }

    async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<Install>> {
        Ok(self
            .installs
            .lock()
            .unwrap()
            .values()
            .find(|i| i.name == name)
            .cloned())
    }

    async fn find_by_token_hash(&self, token_hash: &str) -> anyhow::Result<Option<Install>> {
        Ok(self
            .installs
            .lock()
            .unwrap()
            .values()
            .find(|i| i.token_hash == token_hash)
            .cloned())
    }

    async fn insert(&self, install: &Install) -> anyhow::Result<()> {
        self.installs
            .lock()
            .unwrap()
            .insert(install.id.clone(), install.clone());
        Ok(())
    }

    async fn update_ip(
        &self,
        id: &str,
        ip: &str,
        cf_a_record_id: &str,
        updated_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let mut map = self.installs.lock().unwrap();
        if let Some(install) = map.get_mut(id) {
            install.ip = Some(ip.to_string());
            install.cf_a_record_id = Some(cf_a_record_id.to_string());
            install.updated_at = updated_at;
        }
        Ok(())
    }

    async fn update_acme_record(
        &self,
        id: &str,
        cf_acme_record_id: Option<&str>,
        updated_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let mut map = self.installs.lock().unwrap();
        if let Some(install) = map.get_mut(id) {
            install.cf_acme_record_id = cf_acme_record_id.map(str::to_string);
            install.updated_at = updated_at;
        }
        Ok(())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        self.installs.lock().unwrap().remove(id);
        Ok(())
    }

    async fn count_registrations_from_ip(
        &self,
        remote_ip: &str,
        since: DateTime<Utc>,
    ) -> anyhow::Result<i64> {
        let log = self.log.lock().unwrap();
        let count = log
            .iter()
            .filter(|(ip, created_at)| ip == remote_ip && *created_at > since)
            .count();
        Ok(i64::try_from(count).unwrap_or(i64::MAX))
    }

    async fn log_registration(
        &self,
        remote_ip: &str,
        created_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        self.log
            .lock()
            .unwrap()
            .push((remote_ip.to_string(), created_at));
        Ok(())
    }
}

// ── Mock challenge repository ────────────────────────────────────────────────

pub struct MockChallengeRepository {
    challenges: Mutex<HashMap<String, RegistrationChallenge>>,
}

impl MockChallengeRepository {
    pub fn new() -> Self {
        Self {
            challenges: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl ChallengeRepository for MockChallengeRepository {
    async fn insert(&self, challenge: &RegistrationChallenge) -> anyhow::Result<()> {
        self.challenges
            .lock()
            .unwrap()
            .insert(challenge.id.clone(), challenge.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<RegistrationChallenge>> {
        Ok(self.challenges.lock().unwrap().get(id).cloned())
    }

    async fn consume(&self, id: &str, used_at: DateTime<Utc>) -> anyhow::Result<bool> {
        let mut map = self.challenges.lock().unwrap();
        if let Some(c) = map.get_mut(id) {
            if c.used_at.is_none() {
                c.used_at = Some(used_at);
                return Ok(true);
            }
            return Ok(false);
        }
        Ok(false)
    }

    async fn count_from_ip(&self, remote_ip: &str, since: DateTime<Utc>) -> anyhow::Result<i64> {
        let map = self.challenges.lock().unwrap();
        let count = map
            .values()
            .filter(|c| c.remote_ip == remote_ip && c.created_at > since)
            .count();
        Ok(i64::try_from(count).unwrap_or(i64::MAX))
    }
}
