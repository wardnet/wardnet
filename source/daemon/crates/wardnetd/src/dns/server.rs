use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
use wardnet_common::dns::{DnsConfig, DnsProtocol, FilterAction, UpstreamDns};
use wardnetd_data::repository::QueryLogRow;
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
/// per-device pipeline.
pub struct UdpDnsServer {
    config: Arc<RwLock<DnsConfig>>,
    cache: Arc<RwLock<DnsCache>>,
    dns_filter: Arc<dyn DnsFilterService>,
    bind_addr: SocketAddr,
    injected_socket: Option<Arc<dyn DnsSocket>>,
    running: Arc<AtomicBool>,
    cancel: Mutex<CancellationToken>,
    handle: Mutex<Option<JoinHandle<()>>>,
    // Per-query handlers are tracked so `stop()` can await them. Without
    // this, the spawned handlers keep Arc clones of the bound UDP socket
    // alive past `stop()` and the next `start()` races EADDRINUSE.
    query_tracker: Mutex<Option<TaskTracker>>,
    local_addr: Arc<std::sync::Mutex<Option<SocketAddr>>>,
    log_sink: Option<Arc<DnsLogSink>>,
}

impl UdpDnsServer {
    #[must_use]
    pub fn new(config: DnsConfig, dns_filter: Arc<dyn DnsFilterService>) -> Self {
        Self::with_bind_addr(config, SocketAddr::from(([0, 0, 0, 0], 53)), dns_filter)
    }

    #[must_use]
    pub fn with_bind_addr(
        config: DnsConfig,
        bind_addr: SocketAddr,
        dns_filter: Arc<dyn DnsFilterService>,
    ) -> Self {
        let cache_capacity = config.cache_size as usize;
        Self {
            config: Arc::new(RwLock::new(config)),
            cache: Arc::new(RwLock::new(DnsCache::new(cache_capacity))),
            dns_filter,
            bind_addr,
            injected_socket: None,
            running: Arc::new(AtomicBool::new(false)),
            cancel: Mutex::new(CancellationToken::new()),
            handle: Mutex::new(None),
            query_tracker: Mutex::new(None),
            local_addr: Arc::new(std::sync::Mutex::new(None)),
            log_sink: None,
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
            server_loop(socket, config, cache, dns_filter, log_sink, cancel, tracker).await;
            running.store(false, Ordering::SeqCst);
        });

        *self.handle.lock().await = Some(handle);
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
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
) -> anyhow::Result<()> {
    let request = Message::from_bytes(packet)?;
    let id = request.metadata.id;

    let Some(question) = request.queries.first() else {
        return Ok(());
    };

    let domain = question.name().to_string();
    let rtype = question.query_type();
    let start = std::time::Instant::now();

    // 1. Cache.
    {
        let mut cache_guard = cache.write().await;
        if let Some(cached) = cache_guard.get(&domain, rtype) {
            let mut response = cached.clone();
            response.metadata.id = id;
            let bytes = response.to_bytes()?;
            socket.send_to(&bytes, src).await?;
            tracing::trace!(%domain, ?rtype, "cache hit");
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
            use std::net::IpAddr;

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

    // 3. Forward to upstream.
    let resolver_guard: tokio::sync::RwLockReadGuard<'_, TokioResolver> = resolver.read().await;
    let lookup: Result<Lookup, _> = resolver_guard.lookup(&domain, rtype).await;

    let cfg = config.read().await;
    let upstream = upstream_label(&cfg.upstream_servers);

    // If filtering would have blocked but the kill switch (or the global
    // emergency stop) suppressed it, log this query as `blocked_skipped`
    // — admins can audit what is still being resolved.
    let pass_result = if outcome.would_have_blocked {
        "blocked_skipped"
    } else {
        "forwarded"
    };

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
                    &domain,
                    rtype,
                    response,
                    min_ttl,
                    cfg.cache_ttl_min_secs,
                    cfg.cache_ttl_max_secs,
                );
            }

            let elapsed = start.elapsed();
            tracing::trace!(%domain, ?rtype, ?elapsed, "forwarded");
            record_query(
                log_sink,
                &domain,
                rtype,
                src,
                pass_result,
                upstream.clone(),
                elapsed,
            );
        }
        Err(e) => {
            let mut response = Message::response(id, OpCode::Query);
            response.metadata.recursion_desired = true;
            response.metadata.recursion_available = true;
            response.metadata.response_code = ResponseCode::ServFail;
            response.add_queries(request.queries.clone());

            let bytes = response.to_bytes()?;
            socket.send_to(&bytes, src).await?;

            let elapsed = start.elapsed();
            tracing::debug!(%domain, ?rtype, ?elapsed, error = %e, "upstream failed for {domain}: {e}");
            record_query(
                log_sink,
                &domain,
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
