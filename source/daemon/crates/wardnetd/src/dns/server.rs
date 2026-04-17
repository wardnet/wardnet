use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use hickory_proto::op::{Message, MessageType, OpCode, ResponseCode};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use hickory_proto::xfer::Protocol;
use hickory_resolver::Resolver;
use hickory_resolver::config::{NameServerConfig, ResolveHosts, ResolverConfig, ResolverOpts};
use hickory_resolver::lookup::Lookup;
use hickory_resolver::name_server::TokioConnectionProvider;
use tokio::net::UdpSocket;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use wardnet_common::dns::{DnsConfig, DnsProtocol, FilterAction, UpstreamDns};
use wardnetd_services::dns::cache::DnsCache;
use wardnetd_services::dns::filter::DnsFilter;
use wardnetd_services::dns::server::{DnsServer, DnsSocket};

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

/// Production DNS server that forwards queries to upstream resolvers.
pub struct UdpDnsServer {
    config: Arc<RwLock<DnsConfig>>,
    cache: Arc<RwLock<DnsCache>>,
    filter: Arc<RwLock<DnsFilter>>,
    bind_addr: SocketAddr,
    injected_socket: Option<Arc<dyn DnsSocket>>,
    running: Arc<AtomicBool>,
    cancel: Mutex<CancellationToken>,
    handle: Mutex<Option<JoinHandle<()>>>,
    local_addr: Arc<std::sync::Mutex<Option<SocketAddr>>>,
}

impl UdpDnsServer {
    /// Create a new DNS server that binds to `0.0.0.0:53`.
    #[must_use]
    pub fn new(config: DnsConfig, filter: Arc<RwLock<DnsFilter>>) -> Self {
        Self::with_bind_addr(config, SocketAddr::from(([0, 0, 0, 0], 53)), filter)
    }

    /// Create a new DNS server with a custom bind address.
    #[must_use]
    pub fn with_bind_addr(
        config: DnsConfig,
        bind_addr: SocketAddr,
        filter: Arc<RwLock<DnsFilter>>,
    ) -> Self {
        let cache_capacity = config.cache_size as usize;
        Self {
            config: Arc::new(RwLock::new(config)),
            cache: Arc::new(RwLock::new(DnsCache::new(cache_capacity))),
            filter,
            bind_addr,
            injected_socket: None,
            running: Arc::new(AtomicBool::new(false)),
            cancel: Mutex::new(CancellationToken::new()),
            handle: Mutex::new(None),
            local_addr: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Create a DNS server with a pre-bound socket (for testing).
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_socket(
        config: DnsConfig,
        socket: Arc<dyn DnsSocket>,
        filter: Arc<RwLock<DnsFilter>>,
    ) -> Self {
        let cache_capacity = config.cache_size as usize;
        Self {
            config: Arc::new(RwLock::new(config)),
            cache: Arc::new(RwLock::new(DnsCache::new(cache_capacity))),
            filter,
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 0)),
            injected_socket: Some(socket),
            running: Arc::new(AtomicBool::new(false)),
            cancel: Mutex::new(CancellationToken::new()),
            handle: Mutex::new(None),
            local_addr: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn local_addr(&self) -> Option<SocketAddr> {
        *self.local_addr.lock().expect("local_addr mutex poisoned")
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
        let filter = Arc::clone(&self.filter);
        let running = Arc::clone(&self.running);

        let new_cancel = CancellationToken::new();
        let cancel = new_cancel.clone();
        *self.cancel.lock().await = new_cancel;

        running.store(true, Ordering::SeqCst);

        let handle = tokio::spawn(async move {
            server_loop(socket, config, cache, filter, cancel).await;
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

// ---------------------------------------------------------------------------
// Server loop
// ---------------------------------------------------------------------------

/// Main DNS server event loop.
async fn server_loop(
    socket: Arc<dyn DnsSocket>,
    config: Arc<RwLock<DnsConfig>>,
    cache: Arc<RwLock<DnsCache>>,
    filter: Arc<RwLock<DnsFilter>>,
    cancel: CancellationToken,
) {
    let mut buf = vec![0u8; 4096];

    // Build the initial resolver from config.
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
                        let filter = Arc::clone(&filter);
                        let resolver = Arc::clone(&resolver);

                        tokio::spawn(async move {
                            if let Err(e) = handle_query(
                                &packet, src, &socket, &config, &cache, &filter, &resolver,
                            ).await {
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

/// Handle a single DNS query: parse, check cache, filter, forward, cache response.
#[allow(clippy::too_many_lines)]
async fn handle_query(
    packet: &[u8],
    src: SocketAddr,
    socket: &Arc<dyn DnsSocket>,
    config: &Arc<RwLock<DnsConfig>>,
    cache: &Arc<RwLock<DnsCache>>,
    filter: &Arc<RwLock<DnsFilter>>,
    resolver: &Arc<RwLock<TokioResolver>>,
) -> anyhow::Result<()> {
    let request = Message::from_bytes(packet)?;
    let id = request.id();

    // Extract the first question.
    let Some(question) = request.queries().first() else {
        return Ok(());
    };

    let domain = question.name().to_string();
    let rtype = question.query_type();
    let start = std::time::Instant::now();

    // 1. Check cache.
    {
        let mut cache_guard = cache.write().await;
        if let Some(cached) = cache_guard.get(&domain, rtype) {
            let mut response = cached.clone();
            response.set_id(id);
            let bytes = response.to_bytes()?;
            socket.send_to(&bytes, src).await?;
            tracing::trace!(%domain, ?rtype, "cache hit");
            return Ok(());
        }
    }

    // 2. Ad-blocking filter check.
    {
        let cfg = config.read().await;
        if cfg.ad_blocking_enabled {
            let filter_guard = filter.read().await;
            let domain_lower = domain.trim_end_matches('.').to_ascii_lowercase();
            let action = filter_guard.check(&domain_lower, rtype, src.ip());
            drop(filter_guard);
            match action {
                FilterAction::Block => {
                    let mut response = Message::new();
                    response.set_id(id);
                    response.set_message_type(MessageType::Response);
                    response.set_op_code(OpCode::Query);
                    response.set_recursion_desired(true);
                    response.set_recursion_available(true);
                    response.set_response_code(ResponseCode::NXDomain);
                    response.add_queries(request.queries().to_vec());
                    let bytes = response.to_bytes()?;
                    socket.send_to(&bytes, src).await?;
                    tracing::trace!(%domain, ?rtype, "blocked by filter");
                    return Ok(());
                }
                FilterAction::Rewrite { ip } => {
                    use hickory_proto::rr::{
                        Name, RData, Record,
                        rdata::{A, AAAA},
                    };
                    use std::net::IpAddr;

                    let mut response = Message::new();
                    response.set_id(id);
                    response.set_message_type(MessageType::Response);
                    response.set_op_code(OpCode::Query);
                    response.set_recursion_desired(true);
                    response.set_recursion_available(true);
                    response.set_response_code(ResponseCode::NoError);
                    response.add_queries(request.queries().to_vec());

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
                    return Ok(());
                }
                FilterAction::Pass => {}
            }
        }
    }

    // 3. Forward to upstream resolver.
    let resolver_guard: tokio::sync::RwLockReadGuard<'_, TokioResolver> = resolver.read().await;
    let lookup: Result<Lookup, _> = resolver_guard.lookup(&domain, rtype).await;

    let cfg = config.read().await;

    match lookup {
        Ok(lookup) => {
            // Build response message from lookup records.
            let mut response = Message::new();
            response.set_id(id);
            response.set_message_type(MessageType::Response);
            response.set_op_code(OpCode::Query);
            response.set_recursion_desired(true);
            response.set_recursion_available(true);
            response.set_response_code(ResponseCode::NoError);
            response.add_queries(request.queries().to_vec());

            let mut min_ttl = u32::MAX;
            for record in lookup.records() {
                response.add_answer(record.clone());
                min_ttl = min_ttl.min(record.ttl());
            }

            let bytes = response.to_bytes()?;
            socket.send_to(&bytes, src).await?;

            // Cache the response.
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
        }
        Err(e) => {
            // Return SERVFAIL to the client.
            let mut response = Message::new();
            response.set_id(id);
            response.set_message_type(MessageType::Response);
            response.set_op_code(OpCode::Query);
            response.set_recursion_desired(true);
            response.set_recursion_available(true);
            response.set_response_code(ResponseCode::ServFail);
            response.add_queries(request.queries().to_vec());

            let bytes = response.to_bytes()?;
            socket.send_to(&bytes, src).await?;

            let elapsed = start.elapsed();
            tracing::debug!(%domain, ?rtype, ?elapsed, error = %e, "upstream failed for {domain}: {e}");
        }
    }

    Ok(())
}

/// Type alias for the tokio-based resolver.
type TokioResolver = Resolver<TokioConnectionProvider>;

/// Build a `TokioResolver` from the configured upstream servers.
fn build_resolver(upstreams: &[UpstreamDns]) -> TokioResolver {
    let mut resolver_config = ResolverConfig::new();

    for upstream in upstreams {
        let protocol = match upstream.protocol {
            DnsProtocol::Udp => Protocol::Udp,
            DnsProtocol::Tcp => Protocol::Tcp,
            // DoT and DoH require feature flags on hickory — added in Stage 4 (Security).
            // For now, fall back to TCP for encrypted protocols.
            DnsProtocol::Tls | DnsProtocol::Https => {
                tracing::warn!(
                    address = %upstream.address,
                    protocol = ?upstream.protocol,
                    "encrypted DNS not yet enabled, falling back to TCP: address={address}, protocol={protocol:?}",
                    address = upstream.address,
                    protocol = upstream.protocol,
                );
                Protocol::Tcp
            }
        };

        let port = upstream.port.unwrap_or(match upstream.protocol {
            DnsProtocol::Udp | DnsProtocol::Tcp => 53,
            DnsProtocol::Tls => 853,
            DnsProtocol::Https => 443,
        });

        if let Ok(ip) = upstream.address.parse() {
            let ns = NameServerConfig::new(SocketAddr::new(ip, port), protocol);
            resolver_config.add_name_server(ns);
        } else {
            tracing::warn!(address = %upstream.address, "skipping upstream: not a valid IP: address={address}", address = upstream.address);
        }
    }

    // Fall back to Cloudflare if no valid upstreams configured.
    if resolver_config.name_servers().is_empty() {
        tracing::warn!("no valid upstream DNS servers, falling back to Cloudflare 1.1.1.1");
        resolver_config = ResolverConfig::cloudflare();
    }

    let mut opts = ResolverOpts::default();
    opts.cache_size = 0; // We handle caching ourselves.
    opts.use_hosts_file = ResolveHosts::Never;

    TokioResolver::builder_with_config(resolver_config, TokioConnectionProvider::default())
        .with_options(opts)
        .build()
}
