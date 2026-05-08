use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
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
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use uuid::Uuid;
use wardnet_common::dns::{DnsConfig, DnsProtocol, FilterAction, UpstreamDns, UpstreamId};
use wardnetd_data::repository::QueryLogRow;
use wardnetd_data::repository::TunnelRepository;
use wardnetd_services::DnsFilterService;
use wardnetd_services::dns::DnsLogSink;
use wardnetd_services::dns::cache::DnsCache;
use wardnetd_services::dns::server::{DnsServer, DnsSocket};

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
}

/// Cached metadata required to forward a query to a specific tunnel's
/// DNS server with `SO_BINDTODEVICE` set so the upstream packet egresses
/// via the tunnel interface (see issue #342 — without this, the packet
/// follows the default route and leaks plaintext DNS to the ISP).
struct TunnelForwarderInfo {
    interface_name: String,
    upstream: SocketAddr,
}

impl UdpDnsServer {
    #[must_use]
    pub fn new(
        config: DnsConfig,
        dns_filter: Arc<dyn DnsFilterService>,
        routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
        tunnel_repo: Arc<dyn TunnelRepository>,
    ) -> Self {
        Self::with_bind_addr(
            config,
            SocketAddr::from(([0, 0, 0, 0], 53)),
            dns_filter,
            routing_snapshot,
            tunnel_repo,
        )
    }

    #[must_use]
    pub fn with_bind_addr(
        config: DnsConfig,
        bind_addr: SocketAddr,
        dns_filter: Arc<dyn DnsFilterService>,
        routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
        tunnel_repo: Arc<dyn TunnelRepository>,
    ) -> Self {
        let cache_capacity = config.cache_size as usize;
        Self {
            config: Arc::new(RwLock::new(config)),
            cache: Arc::new(RwLock::new(DnsCache::new(cache_capacity))),
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
        let cache = Arc::clone(&self.cache);
        let dns_filter = Arc::clone(&self.dns_filter);
        let running = Arc::clone(&self.running);
        let log_sink = self.log_sink.clone();
        let routing_snapshot = Arc::clone(&self.routing_snapshot);
        let tunnel_repo = Arc::clone(&self.tunnel_repo);
        let tunnel_forwarders = Arc::clone(&self.tunnel_forwarders);

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
                cache,
                dns_filter,
                log_sink,
                cancel,
                tracker,
                routing_snapshot,
                tunnel_repo,
                tunnel_forwarders,
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
        *self.config.write().await = config;
    }
}

#[allow(clippy::too_many_arguments)]
async fn server_loop(
    socket: Arc<dyn DnsSocket>,
    config: Arc<RwLock<DnsConfig>>,
    cache: Arc<RwLock<DnsCache>>,
    dns_filter: Arc<dyn DnsFilterService>,
    log_sink: Option<Arc<DnsLogSink>>,
    cancel: CancellationToken,
    tracker: TaskTracker,
    routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
    tunnel_repo: Arc<dyn TunnelRepository>,
    tunnel_forwarders: Arc<RwLock<HashMap<Uuid, Arc<TunnelForwarderInfo>>>>,
) {
    let mut buf = vec![0u8; 4096];

    let resolver = {
        let cfg = config.read().await;
        build_resolver(&cfg.upstream_servers)
    };
    let resolver = Arc::new(RwLock::new(resolver));

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
                        let cache = Arc::clone(&cache);
                        let dns_filter = Arc::clone(&dns_filter);
                        let resolver = Arc::clone(&resolver);
                        let log_sink = log_sink.clone();
                        let routing_snapshot = Arc::clone(&routing_snapshot);
                        let tunnel_repo = Arc::clone(&tunnel_repo);
                        let tunnel_forwarders = Arc::clone(&tunnel_forwarders);

                        // Tracker.spawn keeps the Arc<DnsSocket> clone in
                        // this task observable to `stop()`, which awaits
                        // the tracker before returning.
                        tracker.spawn(async move {
                            if let Err(e) = handle_query(
                                &packet,
                                src,
                                &socket,
                                &config,
                                &cache,
                                dns_filter.as_ref(),
                                &resolver,
                                log_sink.as_deref(),
                                &routing_snapshot,
                                &tunnel_repo,
                                &tunnel_forwarders,
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
    cache: &Arc<RwLock<DnsCache>>,
    dns_filter: &dyn DnsFilterService,
    resolver: &Arc<RwLock<TokioResolver>>,
    log_sink: Option<&DnsLogSink>,
    routing_snapshot: &Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
    tunnel_repo: &Arc<dyn TunnelRepository>,
    tunnel_forwarders: &Arc<RwLock<HashMap<Uuid, Arc<TunnelForwarderInfo>>>>,
) -> anyhow::Result<()> {
    let request = Message::from_bytes(packet)?;
    let id = request.metadata.id;

    let Some(question) = request.queries.first() else {
        return Ok(());
    };

    let domain = question.name().to_string();
    let rtype = question.query_type();
    let start = std::time::Instant::now();

    // 0. Resolve upstream pool for this client. Lookup miss = `Default`,
    //    so LAN devices and tunneled devices with override disabled both
    //    fall through to the system-wide upstream — the issue #342 fix
    //    only re-routes the device IPs the routing service has explicitly
    //    paired with `Tunnel(_)`.
    let upstream_id = routing_snapshot
        .load()
        .get(&src.ip())
        .copied()
        .unwrap_or(UpstreamId::Default);

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
    let domain_lower = domain.trim_end_matches('.').to_ascii_lowercase();
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

    // 3. Forward to upstream — choice based on `upstream_id`.
    let pass_result = if outcome.would_have_blocked {
        "blocked_skipped"
    } else {
        "forwarded"
    };

    match upstream_id {
        UpstreamId::Default => {
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
            let mut response = Message::response(id, OpCode::Query);
            response.metadata.recursion_desired = true;
            response.metadata.recursion_available = true;
            response.add_queries(request.queries.clone());

            let mut min_ttl = u32::MAX;
            for record in lookup.answers() {
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
                    cfg.cache_ttl_min_secs,
                    cfg.cache_ttl_max_secs,
                );
            }

            let elapsed = start.elapsed();
            tracing::trace!(%domain, ?rtype, ?elapsed, ?upstream_id, "forwarded");
            record_query(
                log_sink,
                domain,
                rtype,
                src,
                pass_result,
                upstream.clone(),
                elapsed,
            );
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

    bound.send_to(packet, forwarder.upstream).await?;

    let mut buf = vec![0u8; 4096];
    let recv =
        tokio::time::timeout(std::time::Duration::from_secs(5), bound.recv_from(&mut buf)).await;

    let n = match recv {
        Ok(Ok((n, _))) => n,
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

/// Look up (or build, then cache) the forwarder for a given tunnel.
async fn get_or_build_tunnel_forwarder(
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

type TokioResolver = Resolver<TokioRuntimeProvider>;

fn build_resolver(upstreams: &[UpstreamDns]) -> TokioResolver {
    let mut resolver_config = ResolverConfig::default();

    for upstream in upstreams {
        let mut conn = match upstream.protocol {
            DnsProtocol::Udp => ConnectionConfig::udp(),
            DnsProtocol::Tcp => ConnectionConfig::tcp(),
            DnsProtocol::Tls | DnsProtocol::Https => {
                tracing::warn!(
                    address = %upstream.address,
                    protocol = ?upstream.protocol,
                    "encrypted DNS not yet enabled, falling back to TCP",
                );
                ConnectionConfig::tcp()
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

    TokioResolver::builder_with_config(resolver_config, TokioRuntimeProvider::default())
        .with_options(opts)
        .build()
        .expect("failed to build DNS resolver")
}
