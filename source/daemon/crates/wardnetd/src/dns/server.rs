use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::recursor::{DnssecConfig, DnssecPolicy, Recursor, RecursorOptions};
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::dns::{DnsConfig, DnsResolutionMode, UpstreamDns, UpstreamId, UpstreamLatency};
use wardnet_common::event::WardnetEvent;
use wardnetd_data::repository::TunnelRepository;
use wardnetd_services::DnsFilterService;
use wardnetd_services::RoutingProfileService;
use wardnetd_services::dns::DnsLogSink;
use wardnetd_services::dns::UpstreamHealth;
use wardnetd_services::dns::authoritative::AuthoritativeView;
use wardnetd_services::dns::cache::DnsCache;
use wardnetd_services::dns::server::{DnsServer, DnsSocket};
use wardnetd_services::event::EventPublisher;

use crate::dns::pipeline::{
    ClientIdentity, QueryPipeline, TokioRecursor, TokioResolver, TransportProtocol,
};
use crate::dns::rate_limit::RateLimiter;
use crate::dns::upstream_pool::{UpstreamPool, resolver_for};

// The per-query hot path — `QueryPipeline::handle` and its helpers — lives
// in `pipeline.rs` since the resolve-core extraction (#911). Re-export the
// names that historically lived here so `crate::dns::server::` paths keep
// resolving; all but `duration_to_ms` (used by the latency prober below)
// are only reached from the unit tests these days.
pub(crate) use crate::dns::pipeline::duration_to_ms;
#[cfg(test)]
pub(crate) use crate::dns::pipeline::{
    TunnelForwarderInfo, get_or_build_tunnel_forwarder, handle_recursor_outcome, record_query,
    resolve_via_recursor,
};

// ---------------------------------------------------------------------------
// UdpDnsSocket — production socket impl
// ---------------------------------------------------------------------------

/// Production [`DnsSocket`] backed by a real tokio UDP socket.
pub struct UdpDnsSocket {
    socket: UdpSocket,
}

impl UdpDnsSocket {
    pub async fn bind(addr: SocketAddr) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(addr).await?;
        Ok(Self { socket })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }
}

#[async_trait]
impl DnsSocket for UdpDnsSocket {
    async fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        self.socket.recv_from(buf).await
    }

    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> std::io::Result<usize> {
        self.socket.send_to(buf, target).await
    }
}

// ---------------------------------------------------------------------------
// UdpDnsServer
// ---------------------------------------------------------------------------

/// Production DNS server. Filtering is delegated to a
/// [`DnsFilterService`] handle, which owns the per-source / per-profile /
/// per-device pipeline. Per-query upstream selection is driven by the
/// `device_ip → UpstreamId` snapshot the routing service publishes
/// (issue #342).
pub struct UdpDnsServer {
    /// The transport-independent resolve core (#911). Every piece of
    /// per-query state — config, resolver, recursor, rate limiter, cache,
    /// filter, snapshots, authoritative view — lives on the pipeline, so
    /// the config/lifecycle plumbing below and the spawned per-query
    /// handlers observe the same shared state.
    pipeline: Arc<QueryPipeline>,
    bind_addr: SocketAddr,
    injected_socket: Option<Arc<dyn DnsSocket>>,
    running: Arc<AtomicBool>,
    // Serializes `start()` / `stop()` so concurrent callers (the API
    // toggle handler runs synchronously *and* the `DnsRunner` reacts to
    // the same `DnsConfigChanged` event) can't both pass the
    // `running == false` check and race to bind 0.0.0.0:53. Without this
    // the loser hits EADDRINUSE.
    lifecycle: Mutex<()>,
    cancel: Mutex<CancellationToken>,
    handle: Mutex<Option<JoinHandle<()>>>,
    // Per-query handlers are tracked so `stop()` can await them. Without
    // this, the spawned handlers keep Arc clones of the bound UDP socket
    // alive past `stop()` and the next `start()` races EADDRINUSE.
    query_tracker: Mutex<Option<TaskTracker>>,
    local_addr: Arc<std::sync::Mutex<Option<SocketAddr>>>,
    /// Cancellation token for the background cache-invalidation
    /// subscriber spawned in `new`/`with_bind_addr`. The subscriber
    /// flushes the response cache on every `WardnetEvent::DnsFilterRebuilt`
    /// (issue #341) so a freshly-published filter rebuild takes effect on
    /// the next query rather than after cache TTL expiry. Lives for the
    /// whole process — independent of the DNS `enabled` toggle — so
    /// rebuild events that fire while DNS is disabled don't leave stale
    /// entries when DNS is re-enabled. Cancelled in `Drop`.
    cache_invalidator_cancel: CancellationToken,
    /// Per-upstream reachability and rolling-average latency, produced by the
    /// background prober spawned in `with_bind_addr`. Read by
    /// `upstream_latencies()` for the DNS status response, by the forwarding
    /// ladder's serving order, and by the `dns_upstream_unreachable` detector.
    /// Passed in rather than created here: the anomaly registry is wired
    /// before this server exists (see [`UpstreamHealth`]).
    upstream_health: Arc<UpstreamHealth>,
    /// Cancellation token for the background latency prober. Like the
    /// cache-invalidator, the prober lives for the whole process (independent
    /// of the DNS `enabled` toggle) so latency stays fresh while an admin is
    /// configuring upstreams. Cancelled in `Drop`.
    latency_prober_cancel: CancellationToken,
}

impl Drop for UdpDnsServer {
    fn drop(&mut self) {
        // Signal the cache-invalidation subscriber to exit. The task
        // observes the token and breaks out of its `select!`. Drop
        // doesn't await the join — the bus has already been closed by
        // its publisher (or soon will be) and the task self-terminates.
        self.cache_invalidator_cancel.cancel();
        // Stop the background latency prober.
        self.latency_prober_cancel.cancel();
    }
}

impl UdpDnsServer {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        config: DnsConfig,
        dns_filter: Arc<dyn DnsFilterService>,
        routing_profile: Option<Arc<dyn RoutingProfileService>>,
        routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
        device_routing_snapshot: Arc<ArcSwap<HashMap<Uuid, UpstreamId>>>,
        device_snapshot: Arc<ArcSwap<HashMap<IpAddr, Uuid>>>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        events: Arc<dyn EventPublisher>,
        upstream_health: Arc<UpstreamHealth>,
    ) -> Self {
        Self::with_bind_addr(
            config,
            SocketAddr::from(([0, 0, 0, 0], 53)),
            dns_filter,
            routing_profile,
            routing_snapshot,
            device_routing_snapshot,
            device_snapshot,
            tunnel_repo,
            events,
            upstream_health,
        )
    }

    // `events` is consumed (subscribed once, then dropped) — keeping the
    // by-value signature mirrors the other Arc params and lets call
    // sites read like a plain construction.
    #[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
    #[must_use]
    pub fn with_bind_addr(
        config: DnsConfig,
        bind_addr: SocketAddr,
        dns_filter: Arc<dyn DnsFilterService>,
        routing_profile: Option<Arc<dyn RoutingProfileService>>,
        routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
        device_routing_snapshot: Arc<ArcSwap<HashMap<Uuid, UpstreamId>>>,
        device_snapshot: Arc<ArcSwap<HashMap<IpAddr, Uuid>>>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        events: Arc<dyn EventPublisher>,
        upstream_health: Arc<UpstreamHealth>,
    ) -> Self {
        let cache_capacity = config.cache_size as usize;
        let cache = Arc::new(RwLock::new(DnsCache::new(cache_capacity)));
        let pool = Arc::new(ArcSwap::from_pointee(UpstreamPool::build(&config)));
        // Build the recursor only in recursive mode (the default forwarding
        // mode carries no recursor state).
        let recursor = Arc::new(RwLock::new(
            if config.resolution_mode == DnsResolutionMode::Recursive {
                build_recursor(config.dnssec_enabled)
            } else {
                None
            },
        ));
        let rate_limiter = Arc::new(RateLimiter::new(config.rate_limit_per_second));
        let cache_invalidator_cancel = CancellationToken::new();
        // Subscribe BEFORE spawning so any event published between
        // construction and the task running is buffered into this
        // receiver — `broadcast::Receiver` only drops messages older
        // than its position, not future ones. Subscribing in the spawn
        // body would race rebuild events that land in the same tick.
        let event_rx = events.subscribe();
        spawn_cache_invalidator(
            Arc::clone(&cache),
            event_rx,
            cache_invalidator_cancel.clone(),
        );
        let config = Arc::new(RwLock::new(config));
        let latency_prober_cancel = CancellationToken::new();
        spawn_upstream_latency_prober(
            Arc::clone(&config),
            Arc::clone(&upstream_health),
            Arc::clone(&pool),
            latency_prober_cancel.clone(),
        );
        let pipeline = Arc::new(QueryPipeline::new(
            config,
            pool,
            recursor,
            rate_limiter,
            cache,
            dns_filter,
            routing_profile,
            routing_snapshot,
            device_routing_snapshot,
            device_snapshot,
            tunnel_repo,
        ));
        Self {
            pipeline,
            bind_addr,
            injected_socket: None,
            running: Arc::new(AtomicBool::new(false)),
            lifecycle: Mutex::new(()),
            cancel: Mutex::new(CancellationToken::new()),
            handle: Mutex::new(None),
            query_tracker: Mutex::new(None),
            local_addr: Arc::new(std::sync::Mutex::new(None)),
            cache_invalidator_cancel,
            upstream_health,
            latency_prober_cancel,
        }
    }

    /// Attach the query-log sink. Builder-style, and it must run before
    /// the pipeline is shared (i.e. before `start()` or any second
    /// listener takes a clone) — `Arc::get_mut` enforces that: wiring
    /// that breaks the ordering panics here at construction time instead
    /// of a transport silently running with a sink-less pipeline.
    #[must_use]
    pub fn with_log_sink(mut self, sink: Arc<DnsLogSink>) -> Self {
        Arc::get_mut(&mut self.pipeline)
            .expect("with_log_sink must run before the pipeline is shared")
            .log_sink = Some(sink);
        self
    }

    /// The shared resolve core, for a second transport (the `DoT` `:853`
    /// listener) to serve queries through. Must be taken **after**
    /// [`Self::with_log_sink`] — the builder mutates the pipeline through
    /// `Arc::get_mut` and panics once a clone exists.
    #[must_use]
    pub fn pipeline(&self) -> Arc<QueryPipeline> {
        Arc::clone(&self.pipeline)
    }

    /// Return the local address the server is bound to, if `start()` has
    /// run. Tests use this to discover the ephemeral port the kernel
    /// picked for a `127.0.0.1:0` bind so they can fire UDP traffic at
    /// the running server.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr.lock().ok().and_then(|g| *g)
    }

    /// Test-only: drop the recursor so the recursive dispatch path can be
    /// exercised deterministically. With no recursor, `resolve_via_recursor`
    /// takes the recursor-unavailable branch (fallback to forwarding when
    /// upstreams are set, else SERVFAIL) instead of contacting the real
    /// root servers over the network.
    #[cfg(test)]
    pub(crate) async fn clear_recursor_for_test(&self) {
        *self.pipeline.recursor.write().await = None;
    }
}

#[async_trait]
impl DnsServer for UdpDnsServer {
    async fn start(&self) -> anyhow::Result<()> {
        // Hold the lifecycle guard across the whole start: the
        // running-flag check is otherwise racy against another caller
        // (handler vs runner) doing the same check + bind concurrently.
        let _lifecycle = self.lifecycle.lock().await;
        if self.running.load(Ordering::SeqCst) {
            tracing::warn!("DNS server already running");
            return Ok(());
        }

        let socket: Arc<dyn DnsSocket> = if let Some(ref s) = self.injected_socket {
            Arc::clone(s)
        } else {
            let udp_socket = UdpDnsSocket::bind(self.bind_addr).await.map_err(|e| {
                anyhow::anyhow!("failed to bind DNS socket on {}: {e}", self.bind_addr)
            })?;
            let actual_addr = udp_socket.local_addr()?;
            if let Ok(mut guard) = self.local_addr.lock() {
                *guard = Some(actual_addr);
            }
            tracing::info!(%actual_addr, "DNS server listening on {actual_addr}");
            Arc::new(udp_socket)
        };

        let pipeline = Arc::clone(&self.pipeline);
        let running = Arc::clone(&self.running);

        let new_cancel = CancellationToken::new();
        let cancel = new_cancel.clone();
        *self.cancel.lock().await = new_cancel;

        // Fresh TaskTracker per start session: closed and replaced on each
        // stop, so a previous session's drained tracker doesn't leak into
        // the next.
        let tracker = TaskTracker::new();
        *self.query_tracker.lock().await = Some(tracker.clone());

        running.store(true, Ordering::SeqCst);

        let handle = tokio::spawn(async move {
            server_loop(socket, pipeline, cancel, tracker).await;
            running.store(false, Ordering::SeqCst);
        });

        *self.handle.lock().await = Some(handle);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        // Same lifecycle guard as `start()` so a concurrent start() can't
        // pass running=false while we're tearing the listener down.
        let _lifecycle = self.lifecycle.lock().await;
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.cancel.lock().await.cancel();
        if let Some(handle) = self.handle.lock().await.take() {
            handle.await.ok();
        }
        // Drain any in-flight per-query handlers before returning. Each
        // handler holds an Arc clone of the bound UDP socket; without
        // this drain those clones outlive `stop()` and the next `start()`
        // races EADDRINUSE.
        if let Some(tracker) = self.query_tracker.lock().await.take() {
            tracker.close();
            tracker.wait().await;
        }
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    async fn flush_cache(&self) -> u64 {
        self.pipeline.cache.write().await.flush()
    }

    async fn cache_size(&self) -> u64 {
        self.pipeline.cache.read().await.len() as u64
    }

    async fn cache_hit_rate(&self) -> f64 {
        self.pipeline.cache.read().await.hit_rate()
    }

    async fn update_config(&self, config: DnsConfig) {
        let (upstream_changed, dnssec_changed, rebinding_changed, mode_changed, forwarder_changed) = {
            let prev = self.pipeline.config.read().await;
            (
                prev.upstream_servers != config.upstream_servers,
                prev.dnssec_enabled != config.dnssec_enabled,
                prev.rebinding_protection != config.rebinding_protection,
                prev.resolution_mode != config.resolution_mode,
                prev.forwarder_selection_mode != config.forwarder_selection_mode
                    || prev.single_upstream != config.single_upstream,
            )
        };

        // Rebuild the upstream resolver only when something it depends on
        // changed, so unrelated edits (rebinding, rate limit) don't tear
        // down warm DoT/DoH connections. Applied live — the cache and bound
        // socket survive. A forwarder-mode change alters which servers the pool
        // contains and/or their ordering strategy, so it triggers a rebuild too.
        if upstream_changed || dnssec_changed || forwarder_changed {
            self.pipeline
                .pool
                .store(Arc::new(UpstreamPool::build(&config)));
        }

        // (Re)build or clear the recursor when the mode or DNSSEC toggle
        // changes (Stage 5). Built only in recursive mode.
        if mode_changed || dnssec_changed {
            let new_recursor = if config.resolution_mode == DnsResolutionMode::Recursive {
                build_recursor(config.dnssec_enabled)
            } else {
                None
            };
            *self.pipeline.recursor.write().await = new_recursor;
        }

        // Flush cached answers whose validity depends on the changed
        // policy so a toggle takes effect at once rather than after TTL
        // (e.g. enabling rebinding must not keep serving cached private
        // IPs; changing upstreams/DNSSEC/mode must not serve stale answers).
        if upstream_changed
            || dnssec_changed
            || rebinding_changed
            || mode_changed
            || forwarder_changed
        {
            self.pipeline.cache.write().await.flush();
        }

        // Rate is read lock-free on the hot path; refresh it here.
        self.pipeline
            .rate_limiter
            .set_rate(config.rate_limit_per_second);

        *self.pipeline.config.write().await = config;
    }

    async fn update_authoritative_view(&self, view: AuthoritativeView) {
        self.pipeline.authoritative_view.swap(Arc::new(view));
    }

    async fn invalidate_subtree(&self, domain: &str) {
        let removed = self.pipeline.cache.write().await.invalidate_subtree(domain);
        if removed > 0 {
            tracing::debug!(domain, removed, "evicted DNS cache entries for subtree");
        }
    }

    fn upstream_latencies(&self) -> Vec<UpstreamLatency> {
        self.upstream_health.snapshot().as_ref().clone()
    }
}

async fn server_loop(
    socket: Arc<dyn DnsSocket>,
    pipeline: Arc<QueryPipeline>,
    cancel: CancellationToken,
    tracker: TaskTracker,
) {
    let mut buf = vec![0u8; 4096];

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DNS server shutting down");
                break;
            }
            result = socket.recv_from(&mut buf) => {
                match result {
                    Ok((len, src)) => {
                        let packet = buf[..len].to_vec();
                        let socket = Arc::clone(&socket);
                        let pipeline = Arc::clone(&pipeline);

                        // Tracker.spawn keeps the Arc<DnsSocket> clone in
                        // this task observable to `stop()`, which awaits
                        // the tracker before returning.
                        tracker.spawn(async move {
                            if let Err(e) = pipeline
                                .handle(
                                    &packet,
                                    src,
                                    &socket,
                                    ClientIdentity::Ip(src.ip()),
                                    TransportProtocol::Udp,
                                )
                                .await
                            {
                                tracing::debug!(error = %e, %src, "failed to handle DNS query from {src}: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "DNS socket recv error: {e}");
                    }
                }
            }
        }
    }
}

/// Long-lived background task: subscribe to the event bus and flush the
/// response cache on every `WardnetEvent::DnsFilterRebuilt` (issue #341).
///
/// Lives for the whole `UdpDnsServer` instance — independent of the
/// `enabled` toggle that controls the listener — so a rebuild that
/// fires while DNS is paused doesn't leave the cache stale when the
/// listener comes back. Flush on an empty cache is a no-op, so the
/// always-on cost is nil. Exits cleanly on cancellation (Drop) or when
/// the broadcast bus closes.
///
/// `pub(crate)` so the per-branch unit tests in `dns::tests::server`
/// can drive it directly without standing up a full `UdpDnsServer`.
pub(crate) fn spawn_cache_invalidator(
    cache: Arc<RwLock<DnsCache>>,
    mut event_rx: broadcast::Receiver<WardnetEvent>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                () = cancel.cancelled() => {
                    tracing::debug!("DNS cache invalidator cancelled");
                    break;
                }
                result = event_rx.recv() => {
                    match result {
                        Ok(WardnetEvent::DnsFilterRebuilt { .. }) => {
                            let removed = cache.write().await.flush();
                            if removed > 0 {
                                tracing::debug!(
                                    removed,
                                    "flushed DNS cache after filter rebuild"
                                );
                            }
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            // We may have skipped a rebuild event. Flush
                            // defensively — a stale cache is the bug we're
                            // here to prevent.
                            tracing::warn!(
                                skipped = n,
                                "DNS cache invalidator lagged behind event bus; flushing defensively"
                            );
                            cache.write().await.flush();
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::info!("DNS cache invalidator: event bus closed");
                            break;
                        }
                    }
                }
            }
        }
    })
}

/// How often the background prober measures each upstream's latency.
pub(crate) const LATENCY_PROBE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
/// Per-upstream probe deadline; a probe that doesn't answer in time counts as
/// a miss for that round (see `LATENCY_UNREACHABLE_STREAK`).
const LATENCY_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
/// Benign, universally-served name resolved against each upstream to measure
/// round-trip time. IANA-reserved and stable, so it won't skew one provider.
const LATENCY_PROBE_CANARY: &str = "example.com.";
/// Exponential-moving-average weight for the newest sample. Higher = more
/// reactive; lower = smoother.
const LATENCY_EWMA_ALPHA: f64 = 0.3;
/// Consecutive missed probes before an upstream is reported `reachable=false`.
/// A single dropped UDP packet (routine) must not flap the UI, so we debounce.
const LATENCY_UNREACHABLE_STREAK: u32 = 2;

/// Stable cache key for a probe resolver — distinguishes upstreams that share
/// an address but differ in transport (so an encrypted and a plaintext entry
/// on the same IP don't collide in the reuse cache).
fn latency_probe_key(u: &UpstreamDns) -> String {
    format!(
        "{}#{:?}#{:?}#{:?}",
        u.address, u.protocol, u.port, u.tls_server_name
    )
}

/// Spawn the background per-upstream latency prober.
///
/// Each `LATENCY_PROBE_INTERVAL` tick, it reads the *current* configured
/// upstreams (so config changes are picked up without an event subscription)
/// and — only while the forwarding path is actually serving queries — probes
/// every upstream concurrently, folds the round-trip time into a per-upstream
/// EWMA, and publishes the snapshot into `snapshot` for `upstream_latencies()`.
/// Runs for the process lifetime; exits when `cancel` fires (on `Drop`).
///
/// Notes:
/// - Probes run concurrently, so one slow/unreachable upstream can't stall the
///   others and a whole round is bounded by a single timeout.
/// - Resolvers are reused across ticks so DoT/DoH probes keep warm connections
///   and measure steady-state RTT instead of a fresh handshake each sample.
/// - When DNS is disabled or resolving recursively, no probe traffic is sent.
/// - The first probe is deferred by one full interval so short-lived servers
///   (every unit-test server) never emit real outbound DNS.
pub(crate) fn spawn_upstream_latency_prober(
    config: Arc<RwLock<DnsConfig>>,
    health: Arc<UpstreamHealth>,
    pool: Arc<ArcSwap<UpstreamPool>>,
    cancel: CancellationToken,
) {
    let span = tracing::info_span!("dns_upstream_latency_prober");
    tokio::spawn(
        async move {
            let mut ewma: HashMap<String, f64> = HashMap::new();
            let mut fail_streak: HashMap<String, u32> = HashMap::new();
            // Per-upstream resolvers reused across ticks (see fn doc).
            let mut resolvers: HashMap<String, Arc<TokioResolver>> = HashMap::new();
            let mut cached_dnssec: Option<bool> = None;

            let mut ticker = tokio::time::interval_at(
                tokio::time::Instant::now() + LATENCY_PROBE_INTERVAL,
                LATENCY_PROBE_INTERVAL,
            );
            // A slow round (many timing-out upstreams) must not queue up
            // back-to-back ticks; skip missed ticks rather than bursting.
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    () = cancel.cancelled() => break,
                    _ = ticker.tick() => {}
                }

                let (upstreams, dnssec, active) = {
                    let cfg = config.read().await;
                    (
                        cfg.upstream_servers.clone(),
                        cfg.dnssec_enabled,
                        cfg.enabled && cfg.resolution_mode == DnsResolutionMode::Forwarding,
                    )
                };

                // Don't emit probe traffic to third-party upstreams when the
                // forwarding path isn't serving queries (DNS off, or recursive).
                // Clearing the snapshot rather than leaving the last one means
                // "nothing measured", which is what an unmeasured upstream must
                // look like — a stale `reachable=false` would otherwise keep an
                // upstream evicted, and an anomaly open, over a window in which
                // nothing was ever asked of it.
                if !active {
                    if !health.snapshot().is_empty() {
                        health.publish(Vec::new());
                    }
                    continue;
                }

                // Rebuild all resolvers if the DNSSEC policy changed (their
                // validation setting is baked in at build time).
                if cached_dnssec != Some(dnssec) {
                    resolvers.clear();
                    cached_dnssec = Some(dnssec);
                }

                let results = latency_probe_round(
                    &upstreams,
                    dnssec,
                    &mut ewma,
                    &mut fail_streak,
                    &mut resolvers,
                )
                .await;
                health.publish(results);

                // Make the reachability we just measured load-bearing (#1199).
                // Before this the prober's verdict fed the status endpoint and
                // nothing else, so the daemon could show an upstream as down
                // in the UI while still queueing every query onto it.
                //
                // `rcu` rather than a plain store: `update_config` may have
                // rebuilt the pool from a new upstream list while this round
                // was in flight, and the serving order has to be derived from
                // whichever pool won, not from the one this round started with.
                let cfg = config.read().await.clone();
                let previous = pool.rcu(|current| Arc::new(current.with_serving(&cfg, &health)));
                log_serving_changes(&previous, &pool.load());
            }
            tracing::debug!("upstream latency prober stopped");
        }
        .instrument(span),
    );
}

/// Run one probe round: prune state for removed upstreams, probe every current
/// upstream concurrently over the network, and fold the results into the
/// rolling per-upstream state. Returns the snapshot to publish.
async fn latency_probe_round(
    upstreams: &[UpstreamDns],
    dnssec: bool,
    ewma: &mut HashMap<String, f64>,
    fail_streak: &mut HashMap<String, u32>,
    resolvers: &mut HashMap<String, Arc<TokioResolver>>,
) -> Vec<UpstreamLatency> {
    // Drop cached resolvers for servers no longer configured (the ewma /
    // fail_streak maps are pruned by `fold_probe_outcomes` below).
    resolvers.retain(|k, _| upstreams.iter().any(|u| latency_probe_key(u) == *k));
    let outcomes = probe_upstreams(upstreams, dnssec, resolvers).await;
    fold_probe_outcomes(&outcomes, ewma, fail_streak)
}

/// Measure each upstream's round-trip time concurrently. Returns
/// `(address, Some(rtt_ms))` on a successful canary lookup, `(address, None)`
/// on a miss (timeout, error, or a non-IP address we can't probe). This is the
/// network-touching half of a round; the pure folding lives in
/// [`fold_probe_outcomes`].
pub(crate) async fn probe_upstreams(
    upstreams: &[UpstreamDns],
    dnssec: bool,
    resolvers: &mut HashMap<String, Arc<TokioResolver>>,
) -> Vec<(String, Option<f64>)> {
    // Resolve (or lazily build) a resolver per upstream. This is the only step
    // that touches the shared cache; done before any await so the probe futures
    // below own everything they need.
    let targets: Vec<(String, Option<Arc<TokioResolver>>)> = upstreams
        .iter()
        .map(|u| {
            // Only IP-literal addresses can be probed: `resolver_for` builds
            // no name server for anything else, and a resolver with an empty
            // pool would measure nothing. Report a miss instead.
            let resolver = if u.address.parse::<IpAddr>().is_ok() {
                Some(Arc::clone(
                    resolvers.entry(latency_probe_key(u)).or_insert_with(|| {
                        // The prober's own instance, separate from the one the
                        // ladder serves queries through — see `resolver_for`.
                        Arc::new(resolver_for(u, dnssec, LATENCY_PROBE_TIMEOUT))
                    }),
                ))
            } else {
                None
            };
            (u.address.clone(), resolver)
        })
        .collect();

    futures::future::join_all(targets.into_iter().map(|(addr, resolver)| async move {
        let Some(resolver) = resolver else {
            return (addr, None);
        };
        let start = std::time::Instant::now();
        let ok = matches!(
            tokio::time::timeout(
                LATENCY_PROBE_TIMEOUT,
                resolver.lookup(LATENCY_PROBE_CANARY, hickory_proto::rr::RecordType::A),
            )
            .await,
            Ok(Ok(_))
        );
        (addr, ok.then(|| duration_to_ms(start.elapsed())))
    }))
    .await
}

/// Fold a round's raw probe `outcomes` (`address` → `Some(rtt_ms)` on success,
/// `None` on a miss) into the rolling per-upstream state and produce the
/// snapshot. Pure and synchronous — no network — so it is unit-testable:
///
/// - A success updates the EWMA and resets the failure streak.
/// - A miss increments the failure streak but leaves the last-known EWMA.
/// - `reachable` is `false` only after [`LATENCY_UNREACHABLE_STREAK`]
///   consecutive misses (debounce), so a single dropped packet doesn't flap.
/// - State for addresses absent from `outcomes` (removed upstreams) is pruned.
pub(crate) fn fold_probe_outcomes(
    outcomes: &[(String, Option<f64>)],
    ewma: &mut HashMap<String, f64>,
    fail_streak: &mut HashMap<String, u32>,
) -> Vec<UpstreamLatency> {
    let present = |a: &String| outcomes.iter().any(|(addr, _)| addr == a);
    ewma.retain(|a, _| present(a));
    fail_streak.retain(|a, _| present(a));

    let mut results = Vec::with_capacity(outcomes.len());
    for (addr, rtt) in outcomes {
        match rtt {
            Some(rtt_ms) => {
                fail_streak.insert(addr.clone(), 0);
                let avg = ewma.get(addr).map_or(*rtt_ms, |prev| {
                    LATENCY_EWMA_ALPHA * rtt_ms + (1.0 - LATENCY_EWMA_ALPHA) * prev
                });
                ewma.insert(addr.clone(), avg);
            }
            None => {
                *fail_streak.entry(addr.clone()).or_insert(0) += 1;
            }
        }
        // Debounce: only report unreachable after several consecutive misses;
        // keep serving the last-known latency meanwhile.
        let reachable = fail_streak.get(addr).copied().unwrap_or(0) < LATENCY_UNREACHABLE_STREAK;
        results.push(UpstreamLatency {
            address: addr.clone(),
            avg_latency_ms: ewma.get(addr).copied(),
            reachable,
        });
    }
    results
}

/// IANA root server addresses (a–m) — initial hints for the recursive
/// resolver (Stage 5). Stable, well-known IPs (b updated 2023); the
/// recursor discovers the rest of the namespace from these.
const ROOT_HINTS: &[IpAddr] = &[
    IpAddr::V4(Ipv4Addr::new(198, 41, 0, 4)), // a.root-servers.net
    IpAddr::V4(Ipv4Addr::new(170, 247, 170, 2)), // b
    IpAddr::V4(Ipv4Addr::new(192, 33, 4, 12)), // c
    IpAddr::V4(Ipv4Addr::new(199, 7, 91, 13)), // d
    IpAddr::V4(Ipv4Addr::new(192, 203, 230, 10)), // e
    IpAddr::V4(Ipv4Addr::new(192, 5, 5, 241)), // f
    IpAddr::V4(Ipv4Addr::new(192, 112, 36, 4)), // g
    IpAddr::V4(Ipv4Addr::new(198, 97, 190, 53)), // h
    IpAddr::V4(Ipv4Addr::new(192, 36, 148, 17)), // i
    IpAddr::V4(Ipv4Addr::new(192, 58, 128, 30)), // j
    IpAddr::V4(Ipv4Addr::new(193, 0, 14, 129)), // k
    IpAddr::V4(Ipv4Addr::new(199, 7, 83, 42)), // l
    IpAddr::V4(Ipv4Addr::new(202, 12, 27, 33)), // m
];

/// Build a recursive resolver from the root hints (Stage 5).
/// `dnssec_enabled` selects validating (built-in IANA root trust anchor)
/// vs security-unaware. Returns `None` (logged) on construction failure so
/// the caller can fall back to forwarding.
/// Log which upstreams entered or left the serving set.
///
/// The per-query failure warnings name every server that failed, but they are
/// one line per query; these are one line per transition, which is what an
/// operator scanning a journal for "when did this start" actually wants.
fn log_serving_changes(previous: &UpstreamPool, current: &UpstreamPool) {
    let before: Vec<&str> = previous.serving().iter().map(|e| e.address()).collect();
    let after: Vec<&str> = current.serving().iter().map(|e| e.address()).collect();

    for address in &before {
        if !after.contains(address) {
            tracing::warn!(
                upstream = %address,
                "upstream is unreachable; taking it out of the forwarding rotation"
            );
        }
    }
    for address in &after {
        if !before.contains(address) {
            tracing::info!(
                upstream = %address,
                "upstream is answering again; returning it to the forwarding rotation"
            );
        }
    }
}

pub(crate) fn build_recursor(dnssec_enabled: bool) -> Option<TokioRecursor> {
    let dnssec_policy = if dnssec_enabled {
        DnssecPolicy::ValidateWithStaticKey(DnssecConfig::default())
    } else {
        DnssecPolicy::SecurityUnaware
    };
    match Recursor::new(
        ROOT_HINTS,
        dnssec_policy,
        None,
        RecursorOptions::default(),
        TokioRuntimeProvider::default(),
    ) {
        Ok(recursor) => Some(recursor),
        Err(e) => {
            tracing::error!(error = %e, "failed to build recursive resolver; recursive mode will fall back to forwarding");
            None
        }
    }
}
