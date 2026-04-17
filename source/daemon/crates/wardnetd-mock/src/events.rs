//! Periodic fake event emitter for the mock server.
//!
//! Runs as a background task that publishes a rotating set of synthetic
//! domain events every few seconds. The goal is to make the Activity / Logs
//! UI look alive during web-ui development, not to simulate real behaviour.
//!
//! Only two event variants are emitted right now because they are the only
//! ones whose `serde` shape is safe to fabricate without cascading state
//! changes in the daemon:
//!
//! * [`WardnetEvent::TunnelStatsUpdated`] — fakes `bytes_tx`/`bytes_rx` growth
//!   for each seeded tunnel so the Tunnels page charts scroll.
//! * [`WardnetEvent::DnsServerStarted`] / [`WardnetEvent::DnsServerStopped`]
//!   toggle every minute so the DNS status card animates.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wardnet_common::event::WardnetEvent;
use wardnet_common::tunnel::TunnelStatus;
use wardnetd_services::event::EventPublisher;

/// How often the emitter publishes a batch of fake events.
const EMIT_INTERVAL: Duration = Duration::from_secs(5);

/// Handle for the background event emitter. Call [`FakeEventEmitter::shutdown`]
/// to stop the task cleanly.
pub struct FakeEventEmitter {
    handle: JoinHandle<()>,
    cancel: CancellationToken,
}

impl FakeEventEmitter {
    /// Spawn a background task that periodically publishes fake events.
    ///
    /// `tunnel_ids` should be the IDs returned from [`crate::seed::populate`].
    /// If the list is empty no tunnel-stats events are emitted (only the
    /// DNS toggle cycle continues).
    #[must_use]
    pub fn start(publisher: Arc<dyn EventPublisher>, tunnel_ids: Vec<Uuid>) -> Self {
        let cancel = CancellationToken::new();
        let cancel_child = cancel.clone();
        let tunnel_count = tunnel_ids.len();

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(EMIT_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut tick: u64 = 0;
            // Seed some base counters so byte totals look plausible.
            let mut bytes_tx: Vec<u64> = tunnel_ids.iter().map(|_| 1_024 * 32).collect();
            let mut bytes_rx: Vec<u64> = tunnel_ids.iter().map(|_| 1_024 * 48).collect();

            loop {
                tokio::select! {
                    () = cancel_child.cancelled() => {
                        tracing::debug!("mock event emitter cancelled");
                        break;
                    }
                    _ = ticker.tick() => {
                        tick = tick.wrapping_add(1);

                        for (i, tunnel_id) in tunnel_ids.iter().enumerate() {
                            // Pseudo-random growth: tick * a small prime keeps
                            // the number non-uniform without needing the rng.
                            bytes_tx[i] = bytes_tx[i].saturating_add(1_024 + (tick * 37 % 4_096));
                            bytes_rx[i] = bytes_rx[i].saturating_add(2_048 + (tick * 53 % 8_192));

                            publisher.publish(WardnetEvent::TunnelStatsUpdated {
                                tunnel_id: *tunnel_id,
                                status: TunnelStatus::Down,
                                bytes_tx: bytes_tx[i],
                                bytes_rx: bytes_rx[i],
                                last_handshake: None,
                                timestamp: Utc::now(),
                            });
                        }

                        // Toggle DNS server status every 12 ticks (~1 minute).
                        if tick.is_multiple_of(12) {
                            let event = if (tick / 12).is_multiple_of(2) {
                                WardnetEvent::DnsServerStarted { timestamp: Utc::now() }
                            } else {
                                WardnetEvent::DnsServerStopped { timestamp: Utc::now() }
                            };
                            publisher.publish(event);
                        }

                        tracing::debug!(
                            tick,
                            tunnels = tunnel_ids.len(),
                            "emitted fake events: tick={tick}, tunnels={tunnels}",
                            tunnels = tunnel_ids.len(),
                        );
                    }
                }
            }
        });

        tracing::info!(
            interval_secs = EMIT_INTERVAL.as_secs(),
            tunnels = tunnel_count,
            "mock event emitter started: interval_secs={interval}, tunnels={tun}",
            interval = EMIT_INTERVAL.as_secs(),
            tun = tunnel_count,
        );

        Self { handle, cancel }
    }

    /// Signal the emitter to stop and await its completion.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        if let Err(e) = self.handle.await {
            tracing::warn!(error = %e, "mock event emitter join failed: error={e}");
        }
    }
}
