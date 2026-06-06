//! Tests for the passive-learning hook that populates `garp_router_mac`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use wardnetd_data::repository::{LastShutdownInfo, SystemConfigRepository};
use wardnetd_services::device::packet_capture::{ObservedDevice, PacketSource};

use crate::garp_learning::maybe_record_router_mac;

#[derive(Default)]
struct MockSystemConfigRepository {
    data: Mutex<HashMap<String, String>>,
    set_calls: Mutex<usize>,
}

impl MockSystemConfigRepository {
    fn new() -> Self {
        Self::default()
    }

    fn with_router_ip(self, ip: &str) -> Self {
        self.data
            .lock()
            .unwrap()
            .insert("dhcp_router_ip".to_owned(), ip.to_owned());
        self
    }

    fn with_router_mac(self, mac: &str) -> Self {
        self.data
            .lock()
            .unwrap()
            .insert("garp_router_mac".to_owned(), mac.to_owned());
        self
    }

    fn stored_mac(&self) -> Option<String> {
        self.data.lock().unwrap().get("garp_router_mac").cloned()
    }

    fn set_count(&self) -> usize {
        *self.set_calls.lock().unwrap()
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepository {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.data.lock().unwrap().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        *self.set_calls.lock().unwrap() += 1;
        self.data
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.data.lock().unwrap().remove(key);
        Ok(())
    }

    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(0)
    }

    async fn detect_previous_shutdown(&self) -> anyhow::Result<LastShutdownInfo> {
        Ok(LastShutdownInfo {
            state: wardnet_common::api::LastShutdownState::Unknown,
            at: None,
            acknowledged_at: None,
        })
    }
}

fn obs(mac: &str, ip: &str) -> ObservedDevice {
    ObservedDevice {
        mac: mac.to_owned(),
        ip: ip.to_owned(),
        source: PacketSource::Arp,
    }
}

#[tokio::test]
async fn learns_router_mac_when_obs_matches_dhcp_router_ip() {
    let mock = Arc::new(MockSystemConfigRepository::new().with_router_ip("10.91.0.1"));
    let repo: Arc<dyn SystemConfigRepository> = mock.clone();
    maybe_record_router_mac(&obs("aa:bb:cc:dd:ee:ff", "10.91.0.1"), &repo)
        .await
        .unwrap();
    assert_eq!(mock.stored_mac().as_deref(), Some("aa:bb:cc:dd:ee:ff"));
}

#[tokio::test]
async fn skips_write_when_obs_ip_does_not_match_router_ip() {
    let mock = Arc::new(MockSystemConfigRepository::new().with_router_ip("10.91.0.1"));
    let repo: Arc<dyn SystemConfigRepository> = mock.clone();
    let initial_writes = mock.set_count();
    maybe_record_router_mac(&obs("aa:bb:cc:dd:ee:ff", "10.91.0.42"), &repo)
        .await
        .unwrap();
    assert_eq!(mock.set_count(), initial_writes, "no DB write expected");
    assert_eq!(mock.stored_mac(), None);
}

#[tokio::test]
async fn skips_write_when_router_ip_unset() {
    let mock = Arc::new(MockSystemConfigRepository::new());
    let repo: Arc<dyn SystemConfigRepository> = mock.clone();
    maybe_record_router_mac(&obs("aa:bb:cc:dd:ee:ff", "10.91.0.1"), &repo)
        .await
        .unwrap();
    assert_eq!(mock.stored_mac(), None);
    assert_eq!(mock.set_count(), 0);
}

#[tokio::test]
async fn skips_write_when_router_ip_empty_string() {
    // get() returns Some("") which trims to empty -> early return, no write.
    let mock = Arc::new(MockSystemConfigRepository::new().with_router_ip(""));
    let repo: Arc<dyn SystemConfigRepository> = mock.clone();
    let writes_before = mock.set_count();
    maybe_record_router_mac(&obs("aa:bb:cc:dd:ee:ff", "10.91.0.1"), &repo)
        .await
        .unwrap();
    assert_eq!(mock.stored_mac(), None);
    assert_eq!(mock.set_count(), writes_before);
}

#[tokio::test]
async fn skips_write_when_value_unchanged() {
    // Avoid hot-path log/DB churn when the router has been
    // continuously seen and the stored MAC already matches.
    let mock = Arc::new(
        MockSystemConfigRepository::new()
            .with_router_ip("10.91.0.1")
            .with_router_mac("aa:bb:cc:dd:ee:ff"),
    );
    let repo: Arc<dyn SystemConfigRepository> = mock.clone();
    let writes_before = mock.set_count();
    maybe_record_router_mac(&obs("aa:bb:cc:dd:ee:ff", "10.91.0.1"), &repo)
        .await
        .unwrap();
    assert_eq!(
        mock.set_count(),
        writes_before,
        "duplicate observation should not write"
    );
}

#[tokio::test]
async fn updates_when_router_replaced_at_same_ip() {
    // Decision 2: a router replacement that keeps the same IP must
    // be picked up. The hook fires on every observation (not just
    // state-change variants) and writes when the value differs.
    let mock = Arc::new(
        MockSystemConfigRepository::new()
            .with_router_ip("10.91.0.1")
            .with_router_mac("aa:bb:cc:dd:ee:ff"),
    );
    let repo: Arc<dyn SystemConfigRepository> = mock.clone();
    maybe_record_router_mac(&obs("11:22:33:44:55:66", "10.91.0.1"), &repo)
        .await
        .unwrap();
    assert_eq!(mock.stored_mac().as_deref(), Some("11:22:33:44:55:66"));
}
