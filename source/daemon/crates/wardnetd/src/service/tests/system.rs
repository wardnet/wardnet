use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;

use crate::repository::SystemConfigRepository;
use crate::service::{SystemService, SystemServiceImpl};

// -- Mock repository ------------------------------------------------------

struct MockSystemConfigRepo {
    devices: i64,
    tunnels: i64,
    db_size: u64,
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, _key: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
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
    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(self.db_size)
    }
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn status_returns_correct_values() {
    let svc = SystemServiceImpl::new(
        Arc::new(MockSystemConfigRepo {
            devices: 5,
            tunnels: 2,
            db_size: 8192,
        }),
        Instant::now(),
    );

    let resp = svc.status().await.unwrap();
    assert_eq!(resp.version, env!("WARDNET_VERSION"));
    assert_eq!(resp.device_count, 5);
    assert_eq!(resp.tunnel_count, 2);
    assert_eq!(resp.db_size_bytes, 8192);
    assert!(resp.uptime_seconds < 2); // just created
    assert!(resp.cpu_usage_percent >= 0.0);
    assert!(resp.memory_total_bytes > 0);
    assert!(resp.memory_used_bytes <= resp.memory_total_bytes);
}

#[tokio::test]
async fn status_version_comes_from_compile_time_env() {
    let svc = SystemServiceImpl::new(
        Arc::new(MockSystemConfigRepo {
            devices: 0,
            tunnels: 0,
            db_size: 0,
        }),
        Instant::now(),
    );

    let resp = svc.status().await.unwrap();
    // Version is always the compile-time constant, never "unknown".
    assert_eq!(resp.version, env!("WARDNET_VERSION"));
}
