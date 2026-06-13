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
use wardnetd_data::repository::QueryLogRow;
use wardnetd_services::dns::DnsLogSink;
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
    /// `dns_clients` is the `(device_id, ip)` of every seeded device so fake
    /// queries are attributed to a real device. `capture_target` is the
    /// `(device_id, ip)` of the capture-enabled localhost device; one query
    /// per tick is forced onto it so the user PWA's DNS-events stream stays
    /// lively during local dev.
    pub fn start(
        publisher: Arc<dyn EventPublisher>,
        tunnel_ids: Vec<Uuid>,
        dns_sink: Arc<DnsLogSink>,
        dns_clients: Vec<(String, String)>,
        capture_target: Option<(String, String)>,
    ) -> Self {
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

                        // Emit a handful of fake DNS query events every tick so
                        // the live-tail and stats keep updating during dev.
                        if !dns_clients.is_empty() {
                            emit_fake_dns_queries(
                                &dns_sink,
                                &dns_clients,
                                capture_target.as_ref(),
                                tick,
                            );
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

/// Push a small batch of synthetic DNS queries through the log sink so
/// the live tail, stats endpoint, and dashboard cards stay populated
/// while the mock is running. Deterministic on `tick` for reproducibility.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn emit_fake_dns_queries(
    sink: &DnsLogSink,
    clients: &[(String, String)],
    capture_target: Option<&(String, String)>,
    tick: u64,
) {
    const POPULAR: [&str; 6] = [
        "github.com",
        "youtube.com",
        "wikipedia.org",
        "duckduckgo.com",
        "news.ycombinator.com",
        "reddit.com",
    ];
    const AD_BLOCKED: [&str; 5] = [
        "doubleclick.net",
        "googletagmanager.com",
        "adservice.google.com",
        "ads.facebook.com",
        "tracker.example.net",
    ];
    const CDN: [&str; 4] = [
        "fonts.googleapis.com",
        "cdn.cloudflare.com",
        "edge-mqtt.facebook.com",
        "akamaihd.net",
    ];
    const UPSTREAMS: [&str; 2] = ["1.1.1.1", "8.8.8.8"];

    // Per tick, fire 4 events so the chart actually moves.
    for q in 0..4u64 {
        let seed = tick.wrapping_mul(2_654_435_761).wrapping_add(q);
        // Force the first query of each tick onto the capture-enabled localhost
        // device so its DNS-events stream (consumed by the user PWA) keeps
        // flowing; spread the rest across all seeded clients.
        let client = match (q, capture_target) {
            (0, Some(target)) => target,
            _ => &clients[(seed as usize) % clients.len()],
        };
        let bucket = (seed >> 7) % 10;

        let (domain, result) = if bucket < 2 {
            (
                AD_BLOCKED[(seed >> 11) as usize % AD_BLOCKED.len()].to_owned(),
                "blocked",
            )
        } else if bucket < 5 {
            (
                POPULAR[(seed >> 13) as usize % POPULAR.len()].to_owned(),
                "cache_hit",
            )
        } else if bucket == 9 {
            (
                CDN[(seed >> 17) as usize % CDN.len()].to_owned(),
                "forwarded",
            )
        } else {
            (
                POPULAR[(seed >> 19) as usize % POPULAR.len()].to_owned(),
                "forwarded",
            )
        };

        let latency_ms = match result {
            "cache_hit" => 0.4 + ((seed >> 23) as f64 % 5.0) / 10.0,
            "blocked" => 0.2 + ((seed >> 23) as f64 % 3.0) / 10.0,
            _ => 12.0 + ((seed >> 23) as f64 % 50.0),
        };
        let upstream = if result == "forwarded" {
            Some(UPSTREAMS[(seed >> 29) as usize % UPSTREAMS.len()].to_owned())
        } else {
            None
        };

        sink.record(QueryLogRow {
            timestamp: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            client_ip: client.1.clone(),
            domain,
            query_type: "A".to_owned(),
            result: result.to_owned(),
            upstream,
            latency_ms,
            // Attribute to the real device so the per-device capture pipeline
            // can pick up queries for capture-enabled devices.
            device_id: Some(client.0.clone()),
        });
    }
}
