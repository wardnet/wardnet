use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use wardnet_common::stats::{StatsQueryResponse, StatsTopResponse};
use wardnetd_data::repository::IntradayStatRow;

use crate::auth_context;
use crate::error::AppError;
use crate::stats::buffer::StatsBuffer;
use crate::stats::flush_runner::StatsFlushRunner;
use crate::stats::service::StatsService;

#[derive(Default)]
struct SpyService {
    flush_calls: Mutex<u32>,
    maintenance_calls: Mutex<u32>,
}

impl SpyService {
    fn flush_count(&self) -> u32 {
        *self.flush_calls.lock().unwrap()
    }
    fn maintenance_count(&self) -> u32 {
        *self.maintenance_calls.lock().unwrap()
    }
}

#[async_trait]
impl StatsService for SpyService {
    async fn query(
        &self,
        _q: wardnet_common::stats::StatsQuery,
    ) -> Result<StatsQueryResponse, AppError> {
        unimplemented!()
    }
    async fn top(
        &self,
        _q: wardnet_common::stats::StatsTopQuery,
    ) -> Result<StatsTopResponse, AppError> {
        unimplemented!()
    }
    // The spy guards like the real StatsServiceImpl, so the count
    // assertions below also fail if the runner ever stops establishing
    // an admin AuthContext around its calls.
    async fn run_flush(&self, rows: Vec<IntradayStatRow>) -> anyhow::Result<()> {
        auth_context::require_admin()?;
        if !rows.is_empty() {
            *self.flush_calls.lock().unwrap() += 1;
        }
        Ok(())
    }
    async fn run_maintenance(&self) -> anyhow::Result<()> {
        auth_context::require_admin()?;
        *self.maintenance_calls.lock().unwrap() += 1;
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn startup_runs_maintenance_immediately() {
    let buffer = StatsBuffer::new();
    let service = Arc::new(SpyService::default());
    let runner = StatsFlushRunner::start_with_intervals(
        buffer,
        service.clone() as Arc<dyn StatsService>,
        Duration::from_hours(1),
        Duration::from_hours(1),
        Duration::ZERO,
        &tracing::Span::current(),
    );
    // Yield so the spawned task runs its initial flush_ticker.tick() (first
    // tick fires at T=0) and enters the select loop, where sleep_until(T=0)
    // is already past-due and fires maintenance before we get a chance to
    // cancel. Without this yield, shutdown's cancel() races sleep_until(T=0)
    // and maintenance only runs ~50% of the time.
    tokio::task::yield_now().await;
    runner.shutdown().await;
    assert!(
        service.maintenance_count() >= 1,
        "maintenance must run immediately on startup (with an admin AuthContext established)"
    );
}

#[tokio::test]
async fn shutdown_flushes_non_empty_buffer() {
    let buffer = StatsBuffer::new();
    let service = Arc::new(SpyService::default());
    let runner = StatsFlushRunner::start_with_intervals(
        buffer.clone(),
        service.clone() as Arc<dyn StatsService>,
        Duration::from_hours(1),
        Duration::from_hours(1),
        Duration::ZERO,
        &tracing::Span::current(),
    );
    // Record data after starting so the periodic ticker doesn't race us.
    buffer.record("m", "{}", 1.0, "counter");
    runner.shutdown().await;
    assert!(
        service.flush_count() >= 1,
        "shutdown must trigger a final flush of any buffered rows (with an admin AuthContext established)"
    );
}

#[tokio::test]
async fn periodic_flush_drains_buffer() {
    let buffer = StatsBuffer::new();
    let service = Arc::new(SpyService::default());
    let runner = StatsFlushRunner::start_with_intervals(
        buffer.clone(),
        service.clone() as Arc<dyn StatsService>,
        Duration::from_millis(20),
        Duration::from_hours(1),
        Duration::ZERO,
        &tracing::Span::current(),
    );
    buffer.record("m", "{}", 2.0, "counter");
    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.shutdown().await;
    assert!(
        service.flush_count() >= 1,
        "periodic flush must drain non-empty buffer"
    );
}

#[tokio::test]
async fn empty_buffer_is_not_flushed() {
    let buffer = StatsBuffer::new();
    let service = Arc::new(SpyService::default());
    let runner = StatsFlushRunner::start_with_intervals(
        buffer,
        service.clone() as Arc<dyn StatsService>,
        Duration::from_millis(20),
        Duration::from_hours(1),
        Duration::ZERO,
        &tracing::Span::current(),
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.shutdown().await;
    assert_eq!(
        service.flush_count(),
        0,
        "run_flush must not be called when the buffer is empty"
    );
}
