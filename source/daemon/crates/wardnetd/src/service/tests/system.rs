use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use crate::repository::SystemConfigRepository;
use crate::service::{SystemService, SystemServiceImpl};

// -- Mock repository ------------------------------------------------------

struct MockSystemConfigRepo {
    version: Option<String>,
    devices: i64,
    tunnels: i64,
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        if key == "daemon_version" {
            Ok(self.version.clone())
        } else {
            Ok(None)
        }
    }
    async fn set(&self, _key: &str, _value: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(self.devices)
    }
    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(self.tunnels)
    }
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn status_returns_correct_values() {
    let svc = SystemServiceImpl::new(
        Arc::new(MockSystemConfigRepo {
            version: Some("0.1.0".to_owned()),
            devices: 5,
            tunnels: 2,
        }),
        Instant::now(),
    );

    let resp = svc.status().await.unwrap();
    assert_eq!(resp.version, "0.1.0");
    assert_eq!(resp.device_count, 5);
    assert_eq!(resp.tunnel_count, 2);
    assert!(resp.uptime_seconds < 2); // just created
}

#[tokio::test]
async fn status_no_version_returns_unknown() {
    let svc = SystemServiceImpl::new(
        Arc::new(MockSystemConfigRepo {
            version: None,
            devices: 0,
            tunnels: 0,
        }),
        Instant::now(),
    );

    let resp = svc.status().await.unwrap();
    assert_eq!(resp.version, "unknown");
}
