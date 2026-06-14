use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use chrono::Utc;
use hickory_proto::op::{Message, OpCode, ResponseCode};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use hickory_resolver::Resolver;
use hickory_resolver::config::{
    CLOUDFLARE, ConnectionConfig, NameServerConfig, ResolveHosts, ResolverConfig, ResolverOpts,
};
use hickory_resolver::lookup::Lookup;
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::recursor::{DnssecConfig, DnssecPolicy, Recursor, RecursorOptions};
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock, broadcast};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;
use wardnet_common::dns::{
    DnsConfig, DnsProtocol, DnsRecordType, DnsResolutionMode, FilterAction, UpstreamDns, UpstreamId,
};
use wardnet_common::event::WardnetEvent;
use wardnetd_data::repository::QueryLogRow;
use wardnetd_data::repository::TunnelRepository;
use wardnetd_services::DnsFilterService;
use wardnetd_services::dns::DnsLogSink;
use wardnetd_services::dns::authoritative::{AuthoritativeView, parse_conditional_upstream};
use wardnetd_services::dns::cache::DnsCache;
use wardnetd_services::dns::server::{DnsServer, DnsSocket};
use wardnetd_services::event::EventPublisher;

use wardnet_common::net::is_private_ip;

use crate::dns::rate_limit::RateLimiter;

// ---------------------------------------------------------------------------
// UdpDnsSocket — production socket impl
// ---------------------------------------------------------------------------

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
    config: Arc<RwLock<DnsConfig>>,
    /// Shared upstream resolver. Hoisted onto the struct (rather than being
    /// loop-local) so `update_config` can rebuild and swap it when
    /// upstream servers / DNSSEC / protocol change, without restarting the
    /// listener. Built from the current config; rebuilt on every
    /// `update_config`.
    resolver: Arc<RwLock<TokioResolver>>,
    /// Recursive resolver (Stage 5). `Some` only when
    /// `resolution_mode == Recursive`; rebuilt/cleared in `update_config`
    /// when the mode or the DNSSEC toggle changes, so forwarding mode (the
    /// default) carries no recursor state.
    recursor: Arc<RwLock<Option<TokioRecursor>>>,
    /// Per-client-IP token-bucket rate limiter (Stage 4). Enforced at the
    /// top of `handle_query`; disabled when `rate_limit_per_second == 0`.
    rate_limiter: Arc<RateLimiter>,
    cache: Arc<RwLock<DnsCache>>,
    dns_filter: Arc<dyn DnsFilterService>,
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
    log_sink: Option<Arc<DnsLogSink>>,
    /// Lock-free per-query upstream-selection snapshot, populated by the
    /// routing service. Maps a tunneled-device IP to `Tunnel(_)` only
    /// when that tunnel has `override_default_dns = true`.
    routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
    /// Tunnel repository — needed to translate `Tunnel(uuid)` from the
    /// routing snapshot into the interface name + DNS upstream we should
    /// forward to via `SO_BINDTODEVICE`.
    tunnel_repo: Arc<dyn TunnelRepository>,
    /// Cache of per-tunnel forwarders (interface + upstream addr). Keyed
    /// by tunnel UUID. Lazily populated on first miss; stale entries are
    /// fine — a flipped `override_default_dns` simply changes which
    /// upstream the snapshot points at, not the forwarder behaviour
    /// itself, and unused entries quietly age out at process restart.
    tunnel_forwarders: Arc<RwLock<HashMap<Uuid, Arc<TunnelForwarderInfo>>>>,
    /// Cancellation token for the background cache-invalidation
    /// subscriber spawned in `new`/`with_bind_addr`. The subscriber
    /// flushes the response cache on every `WardnetEvent::DnsFilterRebuilt`
    /// (issue #341) so a freshly-published filter rebuild takes effect on
    /// the next query rather than after cache TTL expiry. Lives for the
    /// whole process — independent of the DNS `enabled` toggle — so
    /// rebuild events that fire while DNS is disabled don't leave stale
    /// entries when DNS is re-enabled. Cancelled in `Drop`.
    cache_invalidator_cancel: CancellationToken,
    /// Lock-free snapshot of enabled local authoritative records and
    /// conditional forwarding rules. Swapped atomically by `update_authoritative_view`
    /// whenever `DnsLocalChanged` fires so each query reads a consistent
    /// view without taking a lock.
    authoritative_view: Arc<ArcSwap<AuthoritativeView>>,
    /// Pool of pre-bound UDP sockets for conditional forwarding. Re-using sockets
    /// avoids the per-query `bind` syscall. Each socket is `connect`-ed to the
    /// target upstream just before use, which also restricts the kernel to
    /// accepting responses only from that peer address (closing the off-path
    /// spoofing window). The pool is capped at [`CONDITIONAL_SOCKET_POOL_SIZE`];
    /// under-capacity sockets are created on demand.
    conditional_socket_pool: Arc<tokio::sync::Mutex<Vec<UdpSocket>>>,
}

/// Maximum number of pre-bound UDP sockets kept in the conditional-forwarding pool.
const CONDITIONAL_SOCKET_POOL_SIZE: usize = 8;

impl Drop for UdpDnsServer {
    fn drop(&mut self) {
        // Signal the cache-invalidation subscriber to exit. The task
        // observes the token and breaks out of its `select!`. Drop
        // doesn't await the join — the bus has already been closed by
        // its publisher (or soon will be) and the task self-terminates.
        self.cache_invalidator_cancel.cancel();
    }
}

/// Cached metadata required to forward a query to a specific tunnel's
/// DNS server with `SO_BINDTODEVICE` set so the upstream packet egresses
/// via the tunnel interface (see issue #342 — without this, the packet
/// follows the default route and leaks plaintext DNS to the ISP).
#[derive(Debug)]
pub(crate) struct TunnelForwarderInfo {
    pub(crate) interface_name: String,
    pub(crate) upstream: SocketAddr,
}

impl UdpDnsServer {
    #[must_use]
    pub fn new(
        config: DnsConfig,
        dns_filter: Arc<dyn DnsFilterService>,
        routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self::with_bind_addr(
            config,
            SocketAddr::from(([0, 0, 0, 0], 53)),
            dns_filter,
            routing_snapshot,
            tunnel_repo,
            events,
        )
    }

    // `events` is consumed (subscribed once, then dropped) — keeping the
    // by-value signature mirrors the other Arc params and lets call
    // sites read like a plain construction.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn with_bind_addr(
        config: DnsConfig,
        bind_addr: SocketAddr,
        dns_filter: Arc<dyn DnsFilterService>,
        routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        let cache_capacity = config.cache_size as usize;
        let cache = Arc::new(RwLock::new(DnsCache::new(cache_capacity)));
        let resolver = Arc::new(RwLock::new(build_resolver(
            &config.upstream_servers,
            config.dnssec_enabled,
        )));
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
        Self {
            config: Arc::new(RwLock::new(config)),
            resolver,
            recursor,
            rate_limiter,
            cache,
            dns_filter,
            bind_addr,
            injected_socket: None,
            running: Arc::new(AtomicBool::new(false)),
            lifecycle: Mutex::new(()),
            cancel: Mutex::new(CancellationToken::new()),
            handle: Mutex::new(None),
            query_tracker: Mutex::new(None),
            local_addr: Arc::new(std::sync::Mutex::new(None)),
            log_sink: None,
            routing_snapshot,
            tunnel_repo,
            tunnel_forwarders: Arc::new(RwLock::new(HashMap::new())),
            cache_invalidator_cancel,
            authoritative_view: Arc::new(ArcSwap::from_pointee(AuthoritativeView::empty())),
            conditional_socket_pool: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    #[must_use]
    pub fn with_log_sink(mut self, sink: Arc<DnsLogSink>) -> Self {
        self.log_sink = Some(sink);
        self
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

        let config = Arc::clone(&self.config);
        let resolver = Arc::clone(&self.resolver);
        let recursor = Arc::clone(&self.recursor);
        let rate_limiter = Arc::clone(&self.rate_limiter);
        let cache = Arc::clone(&self.cache);
        let dns_filter = Arc::clone(&self.dns_filter);
        let running = Arc::clone(&self.running);
        let log_sink = self.log_sink.clone();
        let routing_snapshot = Arc::clone(&self.routing_snapshot);
        let tunnel_repo = Arc::clone(&self.tunnel_repo);
        let tunnel_forwarders = Arc::clone(&self.tunnel_forwarders);
        let authoritative_view = Arc::clone(&self.authoritative_view);
        let conditional_socket_pool = Arc::clone(&self.conditional_socket_pool);

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
            server_loop(
                socket,
                config,
                resolver,
                recursor,
                rate_limiter,
                cache,
                dns_filter,
                log_sink,
                cancel,
                tracker,
                routing_snapshot,
                tunnel_repo,
                tunnel_forwarders,
                authoritative_view,
                conditional_socket_pool,
            )
            .await;
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
        self.cache.write().await.flush()
    }

    async fn cache_size(&self) -> u64 {
        self.cache.read().await.len() as u64
    }

    async fn cache_hit_rate(&self) -> f64 {
        self.cache.read().await.hit_rate()
    }

    async fn update_config(&self, config: DnsConfig) {
        let (upstream_changed, dnssec_changed, rebinding_changed, mode_changed) = {
            let prev = self.config.read().await;
            (
                prev.upstream_servers != config.upstream_servers,
                prev.dnssec_enabled != config.dnssec_enabled,
                prev.rebinding_protection != config.rebinding_protection,
                prev.resolution_mode != config.resolution_mode,
            )
        };

        // Rebuild the upstream resolver only when something it depends on
        // changed, so unrelated edits (rebinding, rate limit) don't tear
        // down warm DoT/DoH connections. Applied live — the cache and
        // bound socket survive.
        if upstream_changed || dnssec_changed {
            let new_resolver = build_resolver(&config.upstream_servers, config.dnssec_enabled);
            *self.resolver.write().await = new_resolver;
        }

        // (Re)build or clear the recursor when the mode or DNSSEC toggle
        // changes (Stage 5). Built only in recursive mode.
        if mode_changed || dnssec_changed {
            let new_recursor = if config.resolution_mode == DnsResolutionMode::Recursive {
                build_recursor(config.dnssec_enabled)
            } else {
                None
            };
            *self.recursor.write().await = new_recursor;
        }

        // Flush cached answers whose validity depends on the changed
        // policy so a toggle takes effect at once rather than after TTL
        // (e.g. enabling rebinding must not keep serving cached private
        // IPs; changing upstreams/DNSSEC/mode must not serve stale answers).
        if upstream_changed || dnssec_changed || rebinding_changed || mode_changed {
            self.cache.write().await.flush();
        }

        // Rate is read lock-free on the hot path; refresh it here.
        self.rate_limiter.set_rate(config.rate_limit_per_second);

        *self.config.write().await = config;
    }

    async fn update_authoritative_view(&self, view: AuthoritativeView) {
        self.authoritative_view.swap(Arc::new(view));
    }

    async fn invalidate_domain(&self, domain: &str) {
        let removed = self.cache.write().await.invalidate_domain(domain);
        if removed > 0 {
            tracing::debug!(domain, removed, "evicted DNS cache entries for domain");
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn server_loop(
    socket: Arc<dyn DnsSocket>,
    config: Arc<RwLock<DnsConfig>>,
    resolver: Arc<RwLock<TokioResolver>>,
    recursor: Arc<RwLock<Option<TokioRecursor>>>,
    rate_limiter: Arc<RateLimiter>,
    cache: Arc<RwLock<DnsCache>>,
    dns_filter: Arc<dyn DnsFilterService>,
    log_sink: Option<Arc<DnsLogSink>>,
    cancel: CancellationToken,
    tracker: TaskTracker,
    routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
    tunnel_repo: Arc<dyn TunnelRepository>,
    tunnel_forwarders: Arc<RwLock<HashMap<Uuid, Arc<TunnelForwarderInfo>>>>,
    authoritative_view: Arc<ArcSwap<AuthoritativeView>>,
    conditional_socket_pool: Arc<tokio::sync::Mutex<Vec<UdpSocket>>>,
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
                        let config = Arc::clone(&config);
                        let rate_limiter = Arc::clone(&rate_limiter);
                        let cache = Arc::clone(&cache);
                        let dns_filter = Arc::clone(&dns_filter);
                        let resolver = Arc::clone(&resolver);
                        let recursor = Arc::clone(&recursor);
                        let log_sink = log_sink.clone();
                        let routing_snapshot = Arc::clone(&routing_snapshot);
                        let tunnel_repo = Arc::clone(&tunnel_repo);
                        let tunnel_forwarders = Arc::clone(&tunnel_forwarders);
                        let authoritative_view = Arc::clone(&authoritative_view);
                        let conditional_socket_pool = Arc::clone(&conditional_socket_pool);

                        // Tracker.spawn keeps the Arc<DnsSocket> clone in
                        // this task observable to `stop()`, which awaits
                        // the tracker before returning.
                        tracker.spawn(async move {
                            if let Err(e) = handle_query(
                                &packet,
                                src,
                                &socket,
                                &config,
                                &rate_limiter,
                                &cache,
                                dns_filter.as_ref(),
                                &resolver,
                                &recursor,
                                log_sink.as_deref(),
                                &routing_snapshot,
                                &tunnel_repo,
                                &tunnel_forwarders,
                                &authoritative_view,
                                &conditional_socket_pool,
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

#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
async fn handle_query(
    packet: &[u8],
    src: SocketAddr,
    socket: &Arc<dyn DnsSocket>,
    config: &Arc<RwLock<DnsConfig>>,
    rate_limiter: &Arc<RateLimiter>,
    cache: &Arc<RwLock<DnsCache>>,
    dns_filter: &dyn DnsFilterService,
    resolver: &Arc<RwLock<TokioResolver>>,
    recursor: &Arc<RwLock<Option<TokioRecursor>>>,
    log_sink: Option<&DnsLogSink>,
    routing_snapshot: &Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
    tunnel_repo: &Arc<dyn TunnelRepository>,
    tunnel_forwarders: &Arc<RwLock<HashMap<Uuid, Arc<TunnelForwarderInfo>>>>,
    authoritative_view: &Arc<ArcSwap<AuthoritativeView>>,
    conditional_socket_pool: &Arc<tokio::sync::Mutex<Vec<UdpSocket>>>,
) -> anyhow::Result<()> {
    use hickory_proto::rr::{Name, Record};

    let request = Message::from_bytes(packet)?;
    let id = request.metadata.id;

    let Some(question) = request.queries.first() else {
        return Ok(());
    };

    let domain = question.name().to_string();
    let rtype = question.query_type();
    let start = std::time::Instant::now();

    // 1. Rate limit (per-client token bucket). 0 disables; shed flooding
    // clients before any resolution work, returning REFUSED. The rate is
    // read lock-free from the limiter (no config lock on the hot path).
    if !rate_limiter.check(src.ip()) {
        send_refused(socket, src, id, &request).await?;
        record_query(
            log_sink,
            &domain,
            rtype,
            src,
            "rate_limited",
            None,
            start.elapsed(),
        );
        tracing::debug!(%domain, %src, "rate-limited DNS query");
        return Ok(());
    }

    // 0. Resolve upstream pool for this client.
    let upstream_id = routing_snapshot
        .load()
        .get(&src.ip())
        .copied()
        .unwrap_or(UpstreamId::Default);

    let domain_lower = domain.trim_end_matches('.').to_ascii_lowercase();
    let view = authoritative_view.load();

    // 0.5. Authoritative answer — BEFORE cache, bypasses filter.
    let is_any = rtype == hickory_proto::rr::RecordType::ANY;
    let our_rtype = rtype_from_hickory(rtype);

    let auth_lookup = if is_any {
        view.lookup_all(&domain_lower)
    } else if let Some(rt) = our_rtype {
        view.lookup(&domain_lower, rt)
    } else {
        None
    };

    if let Some(auth_records) = auth_lookup {
        let name = Name::from_str_relaxed(&domain)?;
        let mut response = Message::response(id, OpCode::Query);
        response.metadata.recursion_desired = true;
        response.metadata.recursion_available = true;
        response.metadata.authoritative = true;
        response.add_queries(request.queries.clone());

        if auth_records.is_empty() {
            // Known domain, no records of this type → NOERROR with AA bit
            // (NODATA). If the name falls inside an authoritative zone, attach
            // that zone's pre-built synthetic SOA so the negative answer is
            // cacheable (cloned, not rebuilt, on the per-query path).
            if let Some(zone) = view.authoritative_zone(&domain_lower)
                && let Some(soa) = &zone.soa
            {
                response.add_authority(soa.clone());
            }
        } else {
            for rec in auth_records {
                match make_rdata(rec.record_type, &rec.value, &name) {
                    Ok(rdata) => {
                        response.add_answer(Record::from_rdata(name.clone(), rec.ttl, rdata));
                    }
                    Err(e) => {
                        tracing::warn!(
                            domain = %domain_lower,
                            record_type = ?rec.record_type,
                            error = %e,
                            "skipping malformed authoritative record"
                        );
                    }
                }
            }

            // For A/AAAA queries on a domain with only a CNAME record,
            // add the CNAME to the answer and the target A record (if
            // present) to the additional section.
            if matches!(our_rtype, Some(DnsRecordType::A | DnsRecordType::Aaaa))
                && response.answers.is_empty()
                && let Some(cname_rec) = view.lookup_cname(&domain_lower)
            {
                let target_domain = cname_rec.value.trim_end_matches('.').to_ascii_lowercase();
                if let Ok(rdata) = make_rdata(DnsRecordType::Cname, &cname_rec.value, &name) {
                    response.add_answer(Record::from_rdata(name.clone(), cname_rec.ttl, rdata));
                }
                // Add target A/AAAA to additional section if in view.
                if let Some(target_rt) = our_rtype
                    && let Some(target_records) = view.lookup(&target_domain, target_rt)
                    && let Ok(target_name) = Name::from_str_relaxed(&cname_rec.value)
                {
                    for t_rec in target_records {
                        if let Ok(rdata) = make_rdata(t_rec.record_type, &t_rec.value, &target_name)
                        {
                            response.add_additional(Record::from_rdata(
                                target_name.clone(),
                                t_rec.ttl,
                                rdata,
                            ));
                        }
                    }
                }
            }
        }

        let bytes = response.to_bytes()?;
        socket.send_to(&bytes, src).await?;
        tracing::trace!(%domain, ?rtype, "authoritative answer");
        record_query(
            log_sink,
            &domain,
            rtype,
            src,
            "authoritative",
            None,
            start.elapsed(),
        );
        return Ok(());
    }

    // 0.6. Conditional forwarding check — applies to queries that fall
    //      through the authoritative path (domain not in local view).
    let conditional_upstream: Option<SocketAddr> =
        view.match_forwarding_rule(&domain_lower).and_then(|r| {
            parse_conditional_upstream(&r.upstream)
                .map_err(|e| {
                    tracing::warn!(
                        upstream = %r.upstream,
                        domain = %domain_lower,
                        error = %e,
                        "malformed conditional forwarding upstream — falling through to default"
                    );
                })
                .ok()
        });

    // 0.7. Zone authority — if the name falls under an enabled authoritative
    //      zone and no conditional-forwarding rule claimed it, the gateway owns
    //      the namespace: answer authoritatively instead of leaking the query
    //      upstream. Capture `name_exists` here too: a query type our enum
    //      doesn't model (e.g. HTTPS/SVCB type 65, CAA) yields `auth_lookup =
    //      None` even when the name has other records, so we must distinguish
    //      a truly-absent name (NXDOMAIN) from an existing one with no record
    //      of this type (NODATA). Returning NXDOMAIN for an existing name would
    //      poison its negative cache via the SOA we attach. The zone apex
    //      itself always "exists" (the zone is authoritative for it), so an
    //      apex query with no record is NODATA, never NXDOMAIN. The pre-built
    //      SOA is cloned out so it survives `drop(view)`.
    let authoritative_negative: Option<(Option<Record>, bool)> = if conditional_upstream.is_none() {
        view.authoritative_zone(&domain_lower).map(|z| {
            let name_exists = view.lookup_all(&domain_lower).is_some() || domain_lower == z.name;
            (z.soa.clone(), name_exists)
        })
    } else {
        None
    };

    // Drop the ArcSwap guard before any await point.
    drop(view);

    if let Some((zone_soa, name_exists)) = authoritative_negative {
        let mut response = Message::response(id, OpCode::Query);
        response.metadata.recursion_desired = true;
        response.metadata.recursion_available = true;
        response.metadata.authoritative = true;
        // NXDOMAIN is a name-level negative ("no such name"); only valid when
        // the name is truly absent. If it exists for some other/unmodeled type
        // this is NODATA (NoError with the SOA in the authority section).
        if !name_exists {
            response.metadata.response_code = ResponseCode::NXDomain;
        }
        response.add_queries(request.queries.clone());
        if let Some(soa) = zone_soa {
            response.add_authority(soa);
        }
        let bytes = response.to_bytes()?;
        socket.send_to(&bytes, src).await?;
        let result = if name_exists {
            "authoritative_nodata"
        } else {
            "authoritative_nxdomain"
        };
        tracing::trace!(%domain, ?rtype, result, "authoritative negative answer: {result}");
        record_query(log_sink, &domain, rtype, src, result, None, start.elapsed());
        return Ok(());
    }

    // 1. Cache, keyed by upstream so a tunneled device's answer doesn't
    //    bleed into a LAN device's lookup of the same domain (or vice
    //    versa).
    {
        let mut cache_guard = cache.write().await;
        if let Some(cached) = cache_guard.get(upstream_id, &domain, rtype) {
            let mut response = cached.clone();
            response.metadata.id = id;
            let bytes = response.to_bytes()?;
            socket.send_to(&bytes, src).await?;
            tracing::trace!(%domain, ?rtype, ?upstream_id, "cache hit");
            record_query(
                log_sink,
                &domain,
                rtype,
                src,
                "cache_hit",
                None,
                start.elapsed(),
            );
            return Ok(());
        }
    }

    // 2. Filter.
    let outcome = dns_filter.check(&domain_lower, rtype, src.ip()).await;

    match outcome.action {
        FilterAction::Block => {
            let mut response = Message::response(id, OpCode::Query);
            response.metadata.recursion_desired = true;
            response.metadata.recursion_available = true;
            response.metadata.response_code = ResponseCode::NXDomain;
            response.add_queries(request.queries.clone());
            let bytes = response.to_bytes()?;
            socket.send_to(&bytes, src).await?;
            tracing::trace!(%domain, ?rtype, "blocked by filter");
            record_query(
                log_sink,
                &domain,
                rtype,
                src,
                "blocked",
                None,
                start.elapsed(),
            );
            return Ok(());
        }
        FilterAction::Rewrite { ip } => {
            use hickory_proto::rr::{
                Name, RData, Record,
                rdata::{A, AAAA},
            };

            let mut response = Message::response(id, OpCode::Query);
            response.metadata.recursion_desired = true;
            response.metadata.recursion_available = true;
            response.add_queries(request.queries.clone());

            let name = Name::from_str_relaxed(&domain)?;
            match ip {
                IpAddr::V4(v4) => {
                    let record = Record::from_rdata(name, 60, RData::A(A(v4)));
                    response.add_answer(record);
                }
                IpAddr::V6(v6) => {
                    let record = Record::from_rdata(name, 60, RData::AAAA(AAAA(v6)));
                    response.add_answer(record);
                }
            }
            let bytes = response.to_bytes()?;
            socket.send_to(&bytes, src).await?;
            tracing::trace!(%domain, ?rtype, %ip, "rewritten by filter");
            record_query(
                log_sink,
                &domain,
                rtype,
                src,
                "rewritten",
                None,
                start.elapsed(),
            );
            return Ok(());
        }
        FilterAction::Pass => {}
    }

    // 3. Forward to upstream — conditional forwarding overrides upstream_id.
    let pass_result = if outcome.would_have_blocked {
        "blocked_skipped"
    } else {
        "forwarded"
    };

    if let Some(cond_upstream) = conditional_upstream {
        if let Err(e) = forward_via_conditional(
            cond_upstream,
            socket,
            cache,
            config,
            log_sink,
            packet,
            &request,
            id,
            src,
            &domain,
            rtype,
            start,
            pass_result,
            upstream_id,
            conditional_socket_pool,
        )
        .await
        {
            tracing::warn!(
                error = %e,
                upstream = %cond_upstream,
                %domain,
                "conditional DNS forward failed; returning ServFail"
            );
            send_servfail(socket, src, id, &request).await?;
            record_query(
                log_sink,
                &domain,
                rtype,
                src,
                "upstream_error",
                Some(cond_upstream.ip().to_string()),
                start.elapsed(),
            );
        }
        return Ok(());
    }

    match upstream_id {
        UpstreamId::Default => {
            let recursive = config.read().await.resolution_mode == DnsResolutionMode::Recursive;
            if recursive {
                resolve_via_recursor(
                    recursor,
                    resolver,
                    socket,
                    config,
                    cache,
                    log_sink,
                    request,
                    id,
                    src,
                    &domain,
                    rtype,
                    start,
                    pass_result,
                    upstream_id,
                )
                .await?;
            } else {
                forward_via_default_resolver(
                    resolver,
                    socket,
                    config,
                    cache,
                    log_sink,
                    request,
                    id,
                    src,
                    &domain,
                    rtype,
                    start,
                    pass_result,
                    upstream_id,
                )
                .await?;
            }
        }
        UpstreamId::Tunnel(tunnel_id) => {
            match get_or_build_tunnel_forwarder(tunnel_forwarders, tunnel_repo, tunnel_id).await {
                Ok(forwarder) => {
                    if let Err(e) = forward_via_tunnel(
                        &forwarder,
                        socket,
                        cache,
                        config,
                        log_sink,
                        packet,
                        &request,
                        id,
                        src,
                        &domain,
                        rtype,
                        start,
                        pass_result,
                        upstream_id,
                    )
                    .await
                    {
                        tracing::warn!(
                            error = %e,
                            tunnel_id = %tunnel_id,
                            %domain,
                            "tunnel-bound DNS forward failed; returning ServFail"
                        );
                        send_servfail(socket, src, id, &request).await?;
                        record_query(
                            log_sink,
                            &domain,
                            rtype,
                            src,
                            "upstream_error",
                            Some(forwarder.upstream.ip().to_string()),
                            start.elapsed(),
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        tunnel_id = %tunnel_id,
                        "could not build tunnel forwarder; returning ServFail"
                    );
                    send_servfail(socket, src, id, &request).await?;
                    record_query(
                        log_sink,
                        &domain,
                        rtype,
                        src,
                        "upstream_error",
                        None,
                        start.elapsed(),
                    );
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn forward_via_default_resolver(
    resolver: &Arc<RwLock<TokioResolver>>,
    socket: &Arc<dyn DnsSocket>,
    config: &Arc<RwLock<DnsConfig>>,
    cache: &Arc<RwLock<DnsCache>>,
    log_sink: Option<&DnsLogSink>,
    request: Message,
    id: u16,
    src: SocketAddr,
    domain: &str,
    rtype: hickory_proto::rr::RecordType,
    start: std::time::Instant,
    pass_result: &str,
    upstream_id: UpstreamId,
) -> anyhow::Result<()> {
    let resolver_guard = resolver.read().await;
    let lookup: Result<Lookup, _> = resolver_guard.lookup(domain, rtype).await;

    let cfg = config.read().await;
    let upstream = upstream_label(&cfg.upstream_servers);

    match lookup {
        Ok(lookup) => {
            send_resolved(
                socket,
                cache,
                log_sink,
                &request,
                id,
                src,
                domain,
                rtype,
                start,
                pass_result,
                upstream_id,
                upstream,
                cfg.rebinding_protection,
                cfg.cache_ttl_min_secs,
                cfg.cache_ttl_max_secs,
                lookup.answers(),
            )
            .await?;
        }
        Err(e) => {
            send_servfail(socket, src, id, &request).await?;
            let elapsed = start.elapsed();
            tracing::debug!(%domain, ?rtype, ?elapsed, error = %e, "upstream failed for {domain}: {e}");
            record_query(
                log_sink,
                domain,
                rtype,
                src,
                "upstream_error",
                upstream,
                elapsed,
            );
        }
    }
    Ok(())
}

/// Shared post-resolution responder for the direct-device upstream paths
/// (forwarding + recursive). Applies DNS rebinding protection, builds and
/// sends the response, caches it, and logs the query. Both paths funnel
/// through here so recursive resolution can't bypass rebinding/caching.
#[allow(clippy::too_many_arguments)]
async fn send_resolved(
    socket: &Arc<dyn DnsSocket>,
    cache: &Arc<RwLock<DnsCache>>,
    log_sink: Option<&DnsLogSink>,
    request: &Message,
    id: u16,
    src: SocketAddr,
    domain: &str,
    rtype: hickory_proto::rr::RecordType,
    start: std::time::Instant,
    pass_result: &str,
    upstream_id: UpstreamId,
    upstream_label: Option<String>,
    rebinding_protection: bool,
    cache_ttl_min_secs: u32,
    cache_ttl_max_secs: u32,
    answers: &[hickory_proto::rr::Record],
) -> anyhow::Result<()> {
    use hickory_proto::rr::RData;

    // DNS rebinding protection (Stage 4): reject answers for an external
    // domain that resolve to a private/internal IP. Authoritative records,
    // `.lan`, and conditional forwarding (which legitimately return private
    // IPs) short-circuit earlier and never reach here.
    if rebinding_protection
        && answers.iter().any(|r| match &r.data {
            RData::A(a) => is_private_ip(IpAddr::V4(a.0)),
            RData::AAAA(a) => is_private_ip(IpAddr::V6(a.0)),
            _ => false,
        })
    {
        // Empty NOERROR (NODATA), not NXDOMAIN: the name exists, we just
        // refuse the private answer.
        let mut response = Message::response(id, OpCode::Query);
        response.metadata.recursion_desired = true;
        response.metadata.recursion_available = true;
        response.add_queries(request.queries.clone());
        let bytes = response.to_bytes()?;
        socket.send_to(&bytes, src).await?;
        record_query(
            log_sink,
            domain,
            rtype,
            src,
            "rebinding_blocked",
            upstream_label,
            start.elapsed(),
        );
        tracing::debug!(%domain, "blocked DNS rebinding: private IP for external domain");
        return Ok(());
    }

    let mut response = Message::response(id, OpCode::Query);
    response.metadata.recursion_desired = true;
    response.metadata.recursion_available = true;
    response.add_queries(request.queries.clone());

    let mut min_ttl = u32::MAX;
    for record in answers {
        response.add_answer(record.clone());
        min_ttl = min_ttl.min(record.ttl);
    }

    let bytes = response.to_bytes()?;
    socket.send_to(&bytes, src).await?;

    if min_ttl < u32::MAX && min_ttl > 0 {
        let mut cache_guard = cache.write().await;
        cache_guard.insert(
            upstream_id,
            domain,
            rtype,
            response,
            min_ttl,
            cache_ttl_min_secs,
            cache_ttl_max_secs,
        );
    }

    let elapsed = start.elapsed();
    tracing::trace!(%domain, ?rtype, ?elapsed, ?upstream_id, "resolved");
    record_query(
        log_sink,
        domain,
        rtype,
        src,
        pass_result,
        upstream_label,
        elapsed,
    );
    Ok(())
}

/// Recursive resolution (Stage 5): resolve a direct-device query from the
/// root servers via the recursor. On recursor failure, fall back to
/// forwarding **only if upstreams are configured**; with none configured
/// (pure recursive) return SERVFAIL rather than leaking to a default
/// public resolver.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn resolve_via_recursor(
    recursor: &Arc<RwLock<Option<TokioRecursor>>>,
    resolver: &Arc<RwLock<TokioResolver>>,
    socket: &Arc<dyn DnsSocket>,
    config: &Arc<RwLock<DnsConfig>>,
    cache: &Arc<RwLock<DnsCache>>,
    log_sink: Option<&DnsLogSink>,
    request: Message,
    id: u16,
    src: SocketAddr,
    domain: &str,
    rtype: hickory_proto::rr::RecordType,
    start: std::time::Instant,
    pass_result: &str,
    upstream_id: UpstreamId,
) -> anyhow::Result<()> {
    let Some(query) = request.queries.first().cloned() else {
        send_servfail(socket, src, id, &request).await?;
        return Ok(());
    };

    let (dnssec_ok, has_upstreams, rebinding, ttl_min, ttl_max) = {
        let cfg = config.read().await;
        (
            cfg.dnssec_enabled,
            !cfg.upstream_servers.is_empty(),
            cfg.rebinding_protection,
            cfg.cache_ttl_min_secs,
            cfg.cache_ttl_max_secs,
        )
    };

    let recursor_result = {
        let guard = recursor.read().await;
        match guard.as_ref() {
            Some(rec) => Some(
                rec.resolve(query, std::time::Instant::now(), dnssec_ok)
                    .await,
            ),
            None => None,
        }
    };

    match recursor_result {
        Some(Ok(msg)) => {
            send_resolved(
                socket,
                cache,
                log_sink,
                &request,
                id,
                src,
                domain,
                rtype,
                start,
                pass_result,
                upstream_id,
                Some("recursive".to_owned()),
                rebinding,
                ttl_min,
                ttl_max,
                &msg.answers,
            )
            .await?;
        }
        other => {
            if let Some(Err(e)) = &other {
                tracing::warn!(%domain, error = %e, "recursive resolution failed");
            } else {
                tracing::warn!(%domain, "recursor unavailable; cannot recurse");
            }
            if has_upstreams {
                forward_via_default_resolver(
                    resolver,
                    socket,
                    config,
                    cache,
                    log_sink,
                    request,
                    id,
                    src,
                    domain,
                    rtype,
                    start,
                    pass_result,
                    upstream_id,
                )
                .await?;
            } else {
                send_servfail(socket, src, id, &request).await?;
                record_query(
                    log_sink,
                    domain,
                    rtype,
                    src,
                    "recursor_failed",
                    None,
                    start.elapsed(),
                );
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn forward_via_tunnel(
    forwarder: &TunnelForwarderInfo,
    socket: &Arc<dyn DnsSocket>,
    cache: &Arc<RwLock<DnsCache>>,
    config: &Arc<RwLock<DnsConfig>>,
    log_sink: Option<&DnsLogSink>,
    packet: &[u8],
    _request: &Message,
    _id: u16,
    src: SocketAddr,
    domain: &str,
    rtype: hickory_proto::rr::RecordType,
    start: std::time::Instant,
    pass_result: &str,
    upstream_id: UpstreamId,
) -> anyhow::Result<()> {
    // Build a fresh ephemeral UDP socket bound to the tunnel interface
    // (`SO_BINDTODEVICE`). One per query keeps response demultiplexing
    // trivial — each socket only ever sees the single response to its
    // own outbound query — and avoids any cross-query state hazard. The
    // syscall cost is negligible relative to the DNS round-trip.
    let bound = bind_socket_to_device(&forwarder.interface_name)?;

    // Connect so the kernel only delivers datagrams from `forwarder.upstream`,
    // rejecting off-path spoofed responses at the socket layer.
    bound.connect(forwarder.upstream).await?;
    bound.send(packet).await?;

    let mut buf = vec![0u8; 4096];
    let recv = tokio::time::timeout(std::time::Duration::from_secs(5), bound.recv(&mut buf)).await;

    let n = match recv {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            return Err(anyhow::anyhow!("tunnel upstream recv error: {e}"));
        }
        Err(_) => {
            return Err(anyhow::anyhow!("tunnel upstream timeout"));
        }
    };
    buf.truncate(n);

    // Forward the upstream's response straight back to the client. We
    // also parse it so we can cache it; on parse failure, just send the
    // raw bytes through and skip caching.
    socket.send_to(&buf, src).await?;

    if let Ok(parsed) = Message::from_bytes(&buf) {
        let mut min_ttl = u32::MAX;
        for record in &parsed.answers {
            min_ttl = min_ttl.min(record.ttl);
        }
        if min_ttl < u32::MAX && min_ttl > 0 {
            let cfg = config.read().await;
            let mut cache_guard = cache.write().await;
            cache_guard.insert(
                upstream_id,
                domain,
                rtype,
                parsed,
                min_ttl,
                cfg.cache_ttl_min_secs,
                cfg.cache_ttl_max_secs,
            );
        }
    }

    let elapsed = start.elapsed();
    tracing::trace!(
        %domain,
        ?rtype,
        ?elapsed,
        ?upstream_id,
        interface = %forwarder.interface_name,
        upstream = %forwarder.upstream,
        "forwarded via tunnel"
    );
    record_query(
        log_sink,
        domain,
        rtype,
        src,
        pass_result,
        Some(forwarder.upstream.ip().to_string()),
        elapsed,
    );
    Ok(())
}

async fn send_servfail(
    socket: &Arc<dyn DnsSocket>,
    src: SocketAddr,
    id: u16,
    request: &Message,
) -> anyhow::Result<()> {
    let mut response = Message::response(id, OpCode::Query);
    response.metadata.recursion_desired = true;
    response.metadata.recursion_available = true;
    response.metadata.response_code = ResponseCode::ServFail;
    response.add_queries(request.queries.clone());
    let bytes = response.to_bytes()?;
    socket.send_to(&bytes, src).await?;
    Ok(())
}

/// Send a REFUSED response — used when a client exceeds its rate limit.
async fn send_refused(
    socket: &Arc<dyn DnsSocket>,
    src: SocketAddr,
    id: u16,
    request: &Message,
) -> anyhow::Result<()> {
    let mut response = Message::response(id, OpCode::Query);
    response.metadata.recursion_desired = true;
    response.metadata.recursion_available = true;
    response.metadata.response_code = ResponseCode::Refused;
    response.add_queries(request.queries.clone());
    let bytes = response.to_bytes()?;
    socket.send_to(&bytes, src).await?;
    Ok(())
}

/// Look up (or build, then cache) the forwarder for a given tunnel.
pub(crate) async fn get_or_build_tunnel_forwarder(
    tunnel_forwarders: &Arc<RwLock<HashMap<Uuid, Arc<TunnelForwarderInfo>>>>,
    tunnel_repo: &Arc<dyn TunnelRepository>,
    tunnel_id: Uuid,
) -> anyhow::Result<Arc<TunnelForwarderInfo>> {
    {
        let guard = tunnel_forwarders.read().await;
        if let Some(f) = guard.get(&tunnel_id) {
            return Ok(Arc::clone(f));
        }
    }

    let tunnel = tunnel_repo
        .find_by_id(&tunnel_id.to_string())
        .await?
        .ok_or_else(|| anyhow::anyhow!("tunnel {tunnel_id} not found"))?;
    let cfg = tunnel_repo
        .find_config_by_id(&tunnel_id.to_string())
        .await?
        .ok_or_else(|| anyhow::anyhow!("tunnel {tunnel_id} has no config"))?;
    let upstream_ip: IpAddr = cfg
        .dns
        .first()
        .ok_or_else(|| anyhow::anyhow!("tunnel {tunnel_id} has no DNS server configured"))?
        .parse()
        .map_err(|e| anyhow::anyhow!("tunnel {tunnel_id} DNS not a valid IP: {e}"))?;

    let info = Arc::new(TunnelForwarderInfo {
        interface_name: tunnel.interface_name,
        upstream: SocketAddr::new(upstream_ip, 53),
    });

    let mut guard = tunnel_forwarders.write().await;
    let entry = guard.entry(tunnel_id).or_insert_with(|| Arc::clone(&info));
    Ok(Arc::clone(entry))
}

/// Build a fresh, non-blocking UDP socket bound to `interface_name` via
/// `SO_BINDTODEVICE` so the upstream packet egresses on the tunnel
/// interface. Linux-only; the daemon's systemd unit grants
/// `CAP_NET_RAW`, which is sufficient for `SO_BINDTODEVICE` on
/// Linux ≥ 5.7.
#[cfg(target_os = "linux")]
fn bind_socket_to_device(interface_name: &str) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(socket2::Protocol::UDP))?;
    socket.set_nonblocking(true)?;
    socket.bind_device(Some(interface_name.as_bytes()))?;
    let bind: SocketAddr = "0.0.0.0:0".parse().expect("hardcoded address");
    socket.bind(&bind.into())?;
    let std_socket: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(std_socket)
}

/// Non-Linux platforms (e.g. macOS hosts running unit tests) do not
/// support `SO_BINDTODEVICE`. Fall back to an unbound socket — this
/// still resolves but does NOT enforce egress via the tunnel, which is
/// fine for tests because the mock tunnel interface doesn't exist.
#[cfg(not(target_os = "linux"))]
fn bind_socket_to_device(_interface_name: &str) -> std::io::Result<UdpSocket> {
    let std_socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    std_socket.set_nonblocking(true)?;
    UdpSocket::from_std(std_socket)
}

pub(crate) fn record_query(
    sink: Option<&DnsLogSink>,
    domain: &str,
    rtype: hickory_proto::rr::RecordType,
    src: SocketAddr,
    result: &str,
    upstream: Option<String>,
    latency: std::time::Duration,
) {
    let Some(sink) = sink else { return };

    let row = QueryLogRow {
        timestamp: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        client_ip: src.ip().to_string(),
        domain: domain.trim_end_matches('.').to_owned(),
        query_type: format!("{rtype:?}"),
        result: result.to_owned(),
        upstream,
        latency_ms: duration_to_ms(latency),
        device_id: None,
    };
    sink.record(row);
}

#[allow(clippy::cast_precision_loss)]
pub(crate) fn duration_to_ms(d: std::time::Duration) -> f64 {
    (d.as_micros() as f64) / 1000.0
}

pub(crate) fn upstream_label(upstreams: &[UpstreamDns]) -> Option<String> {
    upstreams.first().map(|u| u.address.clone())
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

/// Map a hickory [`RecordType`] to our domain [`DnsRecordType`].
/// Returns `None` for record types we don't have local records for
/// (e.g. NS, SOA, PTR — these always fall through to upstream).
fn rtype_from_hickory(rt: hickory_proto::rr::RecordType) -> Option<DnsRecordType> {
    use hickory_proto::rr::RecordType as H;
    match rt {
        H::A => Some(DnsRecordType::A),
        H::AAAA => Some(DnsRecordType::Aaaa),
        H::CNAME => Some(DnsRecordType::Cname),
        H::TXT => Some(DnsRecordType::Txt),
        H::MX => Some(DnsRecordType::Mx),
        H::SRV => Some(DnsRecordType::Srv),
        _ => None,
    }
}

/// Build hickory [`RData`] from a domain record type and raw value string.
fn make_rdata(
    rt: DnsRecordType,
    value: &str,
    _name: &hickory_proto::rr::Name,
) -> anyhow::Result<hickory_proto::rr::RData> {
    use hickory_proto::rr::{
        RData,
        rdata::{A, AAAA, CNAME, MX, SRV, TXT},
    };

    match rt {
        DnsRecordType::A => {
            let ip: std::net::Ipv4Addr = value
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid A record value {value:?}: {e}"))?;
            Ok(RData::A(A(ip)))
        }
        DnsRecordType::Aaaa => {
            let ip: std::net::Ipv6Addr = value
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid AAAA record value {value:?}: {e}"))?;
            Ok(RData::AAAA(AAAA(ip)))
        }
        DnsRecordType::Cname => {
            let target = hickory_proto::rr::Name::from_str_relaxed(value)
                .map_err(|e| anyhow::anyhow!("invalid CNAME target {value:?}: {e}"))?;
            Ok(RData::CNAME(CNAME(target)))
        }
        DnsRecordType::Txt => Ok(RData::TXT(TXT::new(vec![value.to_owned()]))),
        DnsRecordType::Mx => {
            // Format: "<priority> <exchange>" e.g. "10 mail.example.com"
            let (prio_str, exchange_str) = value.split_once(' ').ok_or_else(|| {
                anyhow::anyhow!("MX value must be '<priority> <exchange>', got {value:?}")
            })?;
            let preference: u16 = prio_str
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid MX priority {prio_str:?}: {e}"))?;
            let exchange = hickory_proto::rr::Name::from_str_relaxed(exchange_str)
                .map_err(|e| anyhow::anyhow!("invalid MX exchange {exchange_str:?}: {e}"))?;
            Ok(RData::MX(MX::new(preference, exchange)))
        }
        DnsRecordType::Srv => {
            // Format: "<priority> <weight> <port> <target>"
            let parts: Vec<&str> = value.splitn(4, ' ').collect();
            if parts.len() != 4 {
                return Err(anyhow::anyhow!(
                    "SRV value must be '<priority> <weight> <port> <target>', got {value:?}"
                ));
            }
            let priority: u16 = parts[0]
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid SRV priority: {e}"))?;
            let weight: u16 = parts[1]
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid SRV weight: {e}"))?;
            let port: u16 = parts[2]
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid SRV port: {e}"))?;
            let target = hickory_proto::rr::Name::from_str_relaxed(parts[3])
                .map_err(|e| anyhow::anyhow!("invalid SRV target: {e}"))?;
            Ok(RData::SRV(SRV::new(priority, weight, port, target)))
        }
    }
}

/// Forward a DNS query to a conditional upstream (no `SO_BINDTODEVICE`).
///
/// Borrows a socket from `pool` (or creates a new one if the pool is empty),
/// `connect`s it to `upstream` (so the kernel only delivers responses from that
/// peer), sends the query, then validates the response transaction ID and question
/// before forwarding to the client. Without txid validation, a pooled socket that
/// already has a late/duplicate response buffered from a previous query could
/// deliver it to the wrong client.
#[allow(clippy::too_many_arguments)]
async fn forward_via_conditional(
    upstream: SocketAddr,
    socket: &Arc<dyn DnsSocket>,
    cache: &Arc<RwLock<DnsCache>>,
    config: &Arc<RwLock<DnsConfig>>,
    log_sink: Option<&DnsLogSink>,
    packet: &[u8],
    request: &Message,
    id: u16,
    src: SocketAddr,
    domain: &str,
    rtype: hickory_proto::rr::RecordType,
    start: std::time::Instant,
    pass_result: &str,
    upstream_id: UpstreamId,
    pool: &Arc<tokio::sync::Mutex<Vec<UdpSocket>>>,
) -> anyhow::Result<()> {
    // Check out a socket from the pool, or create a new one.
    let bound = {
        let mut guard = pool.lock().await;
        if let Some(s) = guard.pop() {
            s
        } else {
            let std_socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
            std_socket.set_nonblocking(true)?;
            tokio::net::UdpSocket::from_std(std_socket)?
        }
    };

    // connect() restricts the kernel to only accepting datagrams from `upstream`,
    // eliminating off-path response spoofing at the OS layer.
    bound.connect(upstream).await?;
    bound.send(packet).await?;

    let mut buf = vec![0u8; 4096];
    let recv = tokio::time::timeout(std::time::Duration::from_secs(5), bound.recv(&mut buf)).await;

    let n = match recv {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            return_socket_to_pool(pool, bound).await;
            return Err(anyhow::anyhow!("conditional upstream recv error: {e}"));
        }
        Err(_) => {
            return_socket_to_pool(pool, bound).await;
            return Err(anyhow::anyhow!("conditional upstream timeout"));
        }
    };
    buf.truncate(n);

    // Validate the response before forwarding: txid and question must match the
    // outbound query. A pooled socket could in theory receive a late response to a
    // prior query; txid validation ensures stale datagrams are never served or cached.
    if let Ok(parsed) = Message::from_bytes(&buf) {
        if parsed.metadata.id != id {
            return_socket_to_pool(pool, bound).await;
            return Err(anyhow::anyhow!(
                "conditional upstream response txid mismatch (got {}, expected {id})",
                parsed.metadata.id
            ));
        }
        if parsed.queries.first().map(hickory_proto::op::Query::name)
            != request.queries.first().map(hickory_proto::op::Query::name)
        {
            return_socket_to_pool(pool, bound).await;
            return Err(anyhow::anyhow!(
                "conditional upstream response question mismatch"
            ));
        }

        socket.send_to(&buf, src).await?;
        return_socket_to_pool(pool, bound).await;

        let mut min_ttl = u32::MAX;
        for record in &parsed.answers {
            min_ttl = min_ttl.min(record.ttl);
        }
        if min_ttl < u32::MAX && min_ttl > 0 {
            let cfg = config.read().await;
            let mut cache_guard = cache.write().await;
            cache_guard.insert(
                upstream_id,
                domain,
                rtype,
                parsed,
                min_ttl,
                cfg.cache_ttl_min_secs,
                cfg.cache_ttl_max_secs,
            );
        }
    } else {
        // Parse failed — send raw bytes and don't cache.
        socket.send_to(&buf, src).await?;
        return_socket_to_pool(pool, bound).await;
    }

    let elapsed = start.elapsed();
    tracing::trace!(%domain, ?rtype, ?elapsed, %upstream, "forwarded via conditional upstream");
    record_query(
        log_sink,
        domain,
        rtype,
        src,
        pass_result,
        Some(upstream.ip().to_string()),
        elapsed,
    );
    Ok(())
}

async fn return_socket_to_pool(pool: &Arc<tokio::sync::Mutex<Vec<UdpSocket>>>, socket: UdpSocket) {
    let mut guard = pool.lock().await;
    if guard.len() < CONDITIONAL_SOCKET_POOL_SIZE {
        guard.push(socket);
    }
    // If the pool is at capacity, the socket is simply dropped (closed).
}

type TokioResolver = Resolver<TokioRuntimeProvider>;

pub(crate) fn build_resolver(upstreams: &[UpstreamDns], dnssec_enabled: bool) -> TokioResolver {
    let mut resolver_config = ResolverConfig::default();

    for upstream in upstreams {
        let mut conn = match upstream.protocol {
            DnsProtocol::Udp => ConnectionConfig::udp(),
            DnsProtocol::Tcp => ConnectionConfig::tcp(),
            DnsProtocol::Tls | DnsProtocol::Https => {
                // DoT/DoH need an SNI server name for cert validation. The
                // API rejects encrypted upstreams without one; if a bad
                // config slips through, skip the upstream rather than
                // silently downgrade to plaintext.
                let Some(sni) = upstream.tls_server_name.clone() else {
                    tracing::error!(
                        address = %upstream.address,
                        protocol = ?upstream.protocol,
                        "skipping encrypted upstream: tls_server_name is required for DoT/DoH",
                    );
                    continue;
                };
                let sni: Arc<str> = Arc::from(sni);
                match upstream.protocol {
                    DnsProtocol::Https => ConnectionConfig::https(sni, None),
                    _ => ConnectionConfig::tls(sni),
                }
            }
        };

        conn.port = upstream.port.unwrap_or(match upstream.protocol {
            DnsProtocol::Udp | DnsProtocol::Tcp => 53,
            DnsProtocol::Tls => 853,
            DnsProtocol::Https => 443,
        });

        if let Ok(ip) = upstream.address.parse() {
            let ns = NameServerConfig::new(ip, true, vec![conn]);
            resolver_config.add_name_server(ns);
        } else {
            tracing::warn!(address = %upstream.address, "skipping upstream: not a valid IP");
        }
    }

    if resolver_config.name_servers().is_empty() {
        tracing::warn!("no valid upstream DNS servers, falling back to Cloudflare 1.1.1.1");
        resolver_config = ResolverConfig::udp_and_tcp(&CLOUDFLARE);
    }

    let mut opts = ResolverOpts::default();
    opts.cache_size = 0;
    opts.use_hosts_file = ResolveHosts::Never;
    // DNS Stage 4 — local DNSSEC validation (opt-in; default off). hickory
    // validates signatures via the upstream as forwarder and surfaces bogus
    // responses as resolution errors (→ SERVFAIL on the forward path).
    opts.validate = dnssec_enabled;

    TokioResolver::builder_with_config(resolver_config, TokioRuntimeProvider::default())
        .with_options(opts)
        .build()
        .expect("failed to build DNS resolver")
}

type TokioRecursor = Recursor<TokioRuntimeProvider>;

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
