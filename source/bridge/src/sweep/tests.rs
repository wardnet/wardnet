use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::repository::{Install, InstallRepository, NameRepository};

use super::sweep_once;

// ── Mocks ────────────────────────────────────────────────────────────────────

struct MockNameRepo {
    expired: Vec<String>,
}

#[async_trait]
impl NameRepository for MockNameRepo {
    async fn sweep_expired(
        &self,
        _now: DateTime<Utc>,
        _region: &str,
    ) -> anyhow::Result<Vec<String>> {
        Ok(self.expired.clone())
    }

    async fn reserve(
        &self,
        _slug: &str,
        _install_id: &str,
        _region: &str,
        _created_at: DateTime<Utc>,
        _expires_at: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        unimplemented!()
    }
    async fn confirm(&self, _slug: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn release(&self, _slug: &str) -> anyhow::Result<bool> {
        unimplemented!()
    }
    async fn is_taken(&self, _slug: &str) -> anyhow::Result<bool> {
        unimplemented!()
    }
}

struct MockInstallRepo {
    fail_delete: bool,
    deleted: std::sync::Mutex<Vec<String>>,
}

impl MockInstallRepo {
    fn ok() -> Self {
        Self {
            fail_delete: false,
            deleted: std::sync::Mutex::new(vec![]),
        }
    }

    fn failing() -> Self {
        Self {
            fail_delete: true,
            deleted: std::sync::Mutex::new(vec![]),
        }
    }
}

#[async_trait]
impl InstallRepository for MockInstallRepo {
    async fn delete_many(&self, ids: &[String]) -> anyhow::Result<()> {
        if self.fail_delete {
            anyhow::bail!("delete_many injected failure");
        }
        self.deleted.lock().unwrap().extend_from_slice(ids);
        Ok(())
    }

    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Install>> {
        unimplemented!()
    }
    async fn find_by_name(&self, _name: &str) -> anyhow::Result<Option<Install>> {
        unimplemented!()
    }
    async fn find_by_token_hash(&self, _token_hash: &str) -> anyhow::Result<Option<Install>> {
        unimplemented!()
    }
    async fn insert(&self, _install: &Install) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_ip(
        &self,
        _id: &str,
        _ip: &str,
        _cf_a_record_id: &str,
        _updated_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn set_acme_records(
        &self,
        _id: &str,
        _cf_acme_record_ids: &[String],
        _updated_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn delete(&self, _id: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn count_registrations_from_ip(
        &self,
        _ip: &str,
        _since: DateTime<Utc>,
    ) -> anyhow::Result<i64> {
        unimplemented!()
    }
    async fn log_registration(
        &self,
        _remote_ip: &str,
        _created_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn sweep_once_returns_zero_when_nothing_expired() {
    let names = MockNameRepo { expired: vec![] };
    let installs = MockInstallRepo::ok();

    let count = sweep_once(&names, &installs, "use1").await.unwrap();
    assert_eq!(count, 0);
    assert!(installs.deleted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn sweep_once_deletes_expired_installs_and_returns_count() {
    let expired = vec!["abc".to_owned(), "xyz".to_owned()];
    let names = MockNameRepo {
        expired: expired.clone(),
    };
    let installs = MockInstallRepo::ok();

    let count = sweep_once(&names, &installs, "use1").await.unwrap();

    assert_eq!(count, 2);
    let deleted = installs.deleted.lock().unwrap().clone();
    assert_eq!(
        deleted, expired,
        "must delete exactly the swept install ids"
    );
}

#[tokio::test]
async fn sweep_once_swallows_delete_failure_and_still_returns_count() {
    // delete_many fails → error is logged and swallowed; count still reflects
    // the number of name rows swept (they're already gone from the names table).
    let names = MockNameRepo {
        expired: vec!["abc".to_owned()],
    };
    let installs = MockInstallRepo::failing();

    let count = sweep_once(&names, &installs, "use1").await.unwrap();
    assert_eq!(
        count, 1,
        "count must still be 1 even when delete_many fails"
    );
}
