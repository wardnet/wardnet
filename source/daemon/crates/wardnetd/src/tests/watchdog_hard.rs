//! Tests for the ungated hardware watchdog runner (issue #214).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use wardnetd_services::system::WatchdogOps;

use crate::watchdog::HardwareWatchdogRunner;

/// Counts pets and records the disarm, with a configurable availability flag.
struct CountingWatchdog {
    pet_count: AtomicU32,
    disarmed: AtomicBool,
    available: bool,
}

impl CountingWatchdog {
    fn new(available: bool) -> Self {
        Self {
            pet_count: AtomicU32::new(0),
            disarmed: AtomicBool::new(false),
            available,
        }
    }
}

#[async_trait]
impl WatchdogOps for CountingWatchdog {
    async fn pet(&self) {
        self.pet_count.fetch_add(1, Ordering::SeqCst);
    }
    async fn disarm(&self) {
        self.disarmed.store(true, Ordering::SeqCst);
    }
    fn is_available(&self) -> bool {
        self.available
    }
}

#[tokio::test]
async fn pets_on_cadence_and_disarms_on_shutdown() {
    let wd = Arc::new(CountingWatchdog::new(true));
    let parent = tracing::info_span!("test");
    let runner = HardwareWatchdogRunner::start(wd.clone(), Duration::from_millis(50), &parent);

    tokio::time::sleep(Duration::from_millis(220)).await;
    runner.shutdown().await;

    assert!(
        wd.pet_count.load(Ordering::SeqCst) >= 2,
        "expected periodic pets, got {}",
        wd.pet_count.load(Ordering::SeqCst)
    );
    assert!(
        wd.disarmed.load(Ordering::SeqCst),
        "shutdown must disarm the watchdog (magic close) so a clean stop does not reboot",
    );
}

#[tokio::test]
async fn unavailable_device_never_pets_but_still_shuts_down() {
    let wd = Arc::new(CountingWatchdog::new(false));
    let parent = tracing::info_span!("test");
    let runner = HardwareWatchdogRunner::start(wd.clone(), Duration::from_millis(50), &parent);

    tokio::time::sleep(Duration::from_millis(150)).await;
    runner.shutdown().await;

    assert_eq!(
        wd.pet_count.load(Ordering::SeqCst),
        0,
        "an unavailable device must never be pet",
    );
}
