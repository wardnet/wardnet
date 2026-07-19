//! DNS-over-TLS `:853` listener (RFC 7858, issue #912).
//!
//! Serves granted devices' encrypted DNS through the same
//! [`QueryPipeline`] as the UDP `:53` listener. The TLS SNI is both
//! authentication and attribution (the AdGuard/NextDNS industry pattern):
//! a connection must present `<token>.<fqdn>` where `token` is a live
//! per-device grant — the bare apex or an unknown token closes the
//! connection before a single byte of DNS is read (closed resolver; the
//! apex slug is public via CT logs, so apex serving is not acceptable,
//! see #910).
//!
//! ## TLS configuration
//!
//! The listener never owns a certificate. Its `rustls::ServerConfig` is
//! derived per connection from the live `:443` config
//! ([`RustlsConfig::get_inner`]) through a cache keyed on the source
//! config's `Arc`: `reload_from_pem` (the `TlsRenewalRunner` rotation path
//! and the activate/deactivate flow) swaps the inner `Arc` wholesale, so
//! `Arc::ptr_eq` against the retained source is exactly "has the cert
//! rotated". The cache **retains** the source `Arc` so its allocation can
//! never be freed-and-reused under a live cache entry (no ABA). The derived
//! config clears the ALPN list — Android's Private DNS client sends **no**
//! ALPN, and a server that advertises any protocol list would fail those
//! handshakes with `no_application_protocol`.
//!
//! ## Framing
//!
//! RFC 7766 TCP framing: 2-byte big-endian length prefix per message.
//! Queries on one connection are handled concurrently and responses may
//! be written out of order (the message ID correlates them — explicitly
//! allowed by RFC 7766 §7). Connection budget: [`MAX_DOT_CONNS`]
//! concurrent connections, [`MAX_INFLIGHT_QUERIES_PER_CONN`] in-flight
//! queries per connection (backpressure: the read loop parks until a slot
//! frees rather than spawning unboundedly), an idle timeout, a bound on
//! every socket write (a stalled reader must not wedge the connection —
//! or, through the shared lifecycle mutex, daemon shutdown), a per-query
//! bound on the pipeline reply, and a per-**token** rate limit (the
//! transport peer can be a relay's loopback, so per-IP limiting would pool
//! every roaming device into one bucket). A rate breach is answered with
//! `REFUSED`, not a connection reset — tearing the stream down would just
//! make a fail-closed client reconnect-storm.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum_server::tls_rustls::RustlsConfig;
use hickory_proto::op::{Message, OpCode, ResponseCode};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Semaphore, mpsc};
use tokio::task::JoinHandle;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnetd_services::auth_context;
use wardnetd_services::dns::server::DnsSocket;
use wardnetd_services::private_dns::{DotServer, PrivateDnsService};

use crate::dns::pipeline::{ClientIdentity, QueryPipeline, TransportProtocol};
use crate::dns::rate_limit::RateLimiter;
use crate::dns::reply_capture::ReplyCapture;
use crate::tls_server::ServingIdentity;

/// Maximum concurrent `DoT` connections. A Pi-sized budget: each granted
/// phone holds one or two connections, so 64 covers every realistic
/// household with headroom while bounding TLS session memory.
pub(crate) const MAX_DOT_CONNS: usize = 64;

/// Maximum in-flight (spawned, not-yet-answered) queries per connection.
/// The read loop acquires a permit before spawning each query task and
/// parks when the budget is spent, so a client pipelining faster than the
/// pipeline drains applies backpressure instead of growing the task set
/// (and its held query buffers) without bound.
pub(crate) const MAX_INFLIGHT_QUERIES_PER_CONN: usize = 256;

/// TLS handshake must complete within this bound or the connection is
/// dropped — a stalled handshake must not pin one of the connection slots.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Close a connection that has not delivered a complete frame for this
/// long (RFC 7766 idle timeout; clients reconnect transparently). Doubles
/// as the per-write bound: a reader that stalls its receive window must
/// not pin the writer (which would wedge the connection and, via the
/// lifecycle mutex, `stop()` / daemon shutdown).
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

/// Upper bound on one query's trip through the pipeline. The pipeline can
/// legitimately send nothing back (malformed packet), so the reply wait
/// must be bounded rather than trusted.
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);

/// Tracks the live per-device connections' cancellation tokens so a
/// [`DotServer::disconnect_device`] call (fired when an admin revokes a
/// grant) tears the revoked device's established sessions down at once,
/// rather than leaving them authenticated until they happen to reconnect.
#[derive(Default)]
struct ConnRegistry {
    next_id: AtomicU64,
    by_device: std::sync::Mutex<HashMap<Uuid, Vec<(u64, CancellationToken)>>>,
}

impl ConnRegistry {
    fn register(&self, device_id: Uuid, token: CancellationToken) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.by_device
            .lock()
            .expect("conn registry mutex poisoned")
            .entry(device_id)
            .or_default()
            .push((id, token));
        id
    }

    fn deregister(&self, device_id: Uuid, id: u64) {
        let mut map = self.by_device.lock().expect("conn registry mutex poisoned");
        if let Some(conns) = map.get_mut(&device_id) {
            conns.retain(|(conn_id, _)| *conn_id != id);
            if conns.is_empty() {
                map.remove(&device_id);
            }
        }
    }

    fn disconnect(&self, device_id: Uuid) {
        let conns = self
            .by_device
            .lock()
            .expect("conn registry mutex poisoned")
            .remove(&device_id);
        if let Some(conns) = conns {
            for (_, token) in &conns {
                token.cancel();
            }
            tracing::info!(%device_id, count = conns.len(), "closing revoked device's DoT connections");
        }
    }
}

/// The `DoT` `:853` listener. Constructed once in `main.rs` next to the UDP
/// server (whose [`QueryPipeline`] it shares) and driven by the
/// `DotRunner`, which starts it only while Private DNS is enabled and an
/// issued certificate is live.
pub struct DotDnsServer {
    bind_addr: SocketAddr,
    pipeline: Arc<QueryPipeline>,
    /// The live `:443` config — shares its inner `ArcSwap` with the HTTPS
    /// listener, so cert rotations are visible here without any wiring.
    tls: RustlsConfig,
    serving: Arc<dyn ServingIdentity>,
    private_dns: Arc<dyn PrivateDnsService>,
    /// Per-grant-token buckets. The rate is read from the live DNS config
    /// per query (so a config change reaches every open connection), so the
    /// limiter's own atomic rate is unused on this path.
    token_limiter: Arc<RateLimiter<String>>,
    /// Live per-device connections, for revocation-driven teardown.
    registry: Arc<ConnRegistry>,
    running: Arc<AtomicBool>,
    /// Serialises start/stop transitions (same shape as `UdpDnsServer`).
    lifecycle: Mutex<()>,
    cancel: Mutex<CancellationToken>,
    handle: Mutex<Option<JoinHandle<()>>>,
    conn_tracker: Mutex<Option<TaskTracker>>,
    local_addr: Arc<std::sync::Mutex<Option<SocketAddr>>>,
    /// [`IDLE_TIMEOUT`] / [`QUERY_TIMEOUT`] in production; shrunk by tests
    /// so the timeout paths run in milliseconds.
    idle_timeout: Duration,
    query_timeout: Duration,
}

impl DotDnsServer {
    #[must_use]
    pub fn new(
        pipeline: Arc<QueryPipeline>,
        tls: RustlsConfig,
        serving: Arc<dyn ServingIdentity>,
        private_dns: Arc<dyn PrivateDnsService>,
    ) -> Self {
        Self::with_bind_addr(
            SocketAddr::from(([0, 0, 0, 0], 853)),
            pipeline,
            tls,
            serving,
            private_dns,
        )
    }

    #[must_use]
    pub fn with_bind_addr(
        bind_addr: SocketAddr,
        pipeline: Arc<QueryPipeline>,
        tls: RustlsConfig,
        serving: Arc<dyn ServingIdentity>,
        private_dns: Arc<dyn PrivateDnsService>,
    ) -> Self {
        Self {
            bind_addr,
            pipeline,
            tls,
            serving,
            private_dns,
            token_limiter: Arc::new(RateLimiter::new(0)),
            registry: Arc::new(ConnRegistry::default()),
            running: Arc::new(AtomicBool::new(false)),
            lifecycle: Mutex::new(()),
            cancel: Mutex::new(CancellationToken::new()),
            handle: Mutex::new(None),
            conn_tracker: Mutex::new(None),
            local_addr: Arc::new(std::sync::Mutex::new(None)),
            idle_timeout: IDLE_TIMEOUT,
            query_timeout: QUERY_TIMEOUT,
        }
    }

    /// The local address the listener is bound to, if `start()` has run.
    /// Tests use this to discover the kernel-picked ephemeral port.
    #[cfg(test)]
    pub(crate) fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr.lock().ok().and_then(|g| *g)
    }

    /// Test-only: shrink the idle / query timeouts so the timeout paths
    /// complete in test time rather than tens of seconds.
    #[cfg(test)]
    pub(crate) fn with_timeouts(mut self, idle: Duration, query: Duration) -> Self {
        self.idle_timeout = idle;
        self.query_timeout = query;
        self
    }
}

#[async_trait]
impl DotServer for DotDnsServer {
    async fn start(&self) -> anyhow::Result<()> {
        // Lifecycle guard across check + bind, so a concurrent start/stop
        // (runner tick vs shutdown) can't race the running flag.
        let _lifecycle = self.lifecycle.lock().await;
        if self.running.load(Ordering::SeqCst) {
            tracing::warn!("DoT server already running");
            return Ok(());
        }

        let listener = TcpListener::bind(self.bind_addr).await?;
        let local = listener.local_addr()?;
        if let Ok(mut guard) = self.local_addr.lock() {
            *guard = Some(local);
        }

        let cancel = CancellationToken::new();
        *self.cancel.lock().await = cancel.clone();
        let tracker = TaskTracker::new();
        *self.conn_tracker.lock().await = Some(tracker.clone());
        self.running.store(true, Ordering::SeqCst);

        let ctx = ConnContext {
            pipeline: Arc::clone(&self.pipeline),
            serving: Arc::clone(&self.serving),
            private_dns: Arc::clone(&self.private_dns),
            token_limiter: Arc::clone(&self.token_limiter),
            registry: Arc::clone(&self.registry),
            idle_timeout: self.idle_timeout,
            query_timeout: self.query_timeout,
        };
        let acceptor_source = AcceptorSource {
            tls: self.tls.clone(),
            cache: std::sync::Mutex::new(None),
        };
        let span = tracing::info_span!("dot_server");
        let handle = tokio::spawn(
            accept_loop(listener, acceptor_source, ctx, cancel, tracker).instrument(span),
        );
        *self.handle.lock().await = Some(handle);

        tracing::info!(addr = %local, "DoT server listening");
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        let _lifecycle = self.lifecycle.lock().await;
        if !self.running.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.cancel.lock().await.cancel();
        if let Some(handle) = self.handle.lock().await.take() {
            let _ = handle.await;
        }
        // Drain in-flight connections so the next start() can't race the
        // port and clients get their TLS close_notify. The accept-loop
        // cancel is the parent of every connection's token, so each
        // connection winds down promptly rather than blocking the drain.
        if let Some(tracker) = self.conn_tracker.lock().await.take() {
            tracker.close();
            tracker.wait().await;
        }
        self.running.store(false, Ordering::SeqCst);
        tracing::info!("DoT server stopped");
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    fn disconnect_device(&self, device_id: Uuid) {
        self.registry.disconnect(device_id);
    }
}

/// Everything one connection needs, cloned per accept.
#[derive(Clone)]
struct ConnContext {
    pipeline: Arc<QueryPipeline>,
    serving: Arc<dyn ServingIdentity>,
    private_dns: Arc<dyn PrivateDnsService>,
    token_limiter: Arc<RateLimiter<String>>,
    registry: Arc<ConnRegistry>,
    idle_timeout: Duration,
    query_timeout: Duration,
}

/// The accept loop's handle on the live `:443` config plus the cache of the
/// `ServerConfig` derived from it, keyed by the source `Arc`:
/// `(retained source Arc, derived acceptor)`. `reload_from_pem` swaps the
/// source `Arc` wholesale, so `Arc::ptr_eq` against the retained source is
/// exactly "the cert rotated", and the derived config is rebuilt on the
/// next accept. Retaining the source `Arc` keeps its allocation alive for
/// as long as it is cached, so its address can never be reused under a live
/// entry (which a raw pointer key would admit — a stale-cert ABA hazard).
struct AcceptorSource {
    tls: RustlsConfig,
    cache: std::sync::Mutex<Option<(Arc<rustls::ServerConfig>, TlsAcceptor)>>,
}

impl AcceptorSource {
    fn acceptor(&self) -> TlsAcceptor {
        let inner = self.tls.get_inner();
        let mut cache = self.cache.lock().expect("acceptor cache mutex poisoned");
        if let Some((cached_src, acceptor)) = cache.as_ref()
            && Arc::ptr_eq(cached_src, &inner)
        {
            return acceptor.clone();
        }
        let mut derived = (*inner).clone();
        // Empty ALPN: Android's Private DNS client sends none, and rustls
        // fails the handshake with `no_application_protocol` once the
        // server advertises a list. The `:443` source config carries none
        // today either — this keeps DoT correct even if HTTPS ever starts
        // advertising h2.
        derived.alpn_protocols.clear();
        let acceptor = TlsAcceptor::from(Arc::new(derived));
        *cache = Some((inner, acceptor.clone()));
        acceptor
    }
}

async fn accept_loop(
    listener: TcpListener,
    acceptors: AcceptorSource,
    ctx: ConnContext,
    cancel: CancellationToken,
    tracker: TaskTracker,
) {
    let conn_slots = Arc::new(Semaphore::new(MAX_DOT_CONNS));
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = listener.accept() => match result {
                Ok((stream, peer)) => {
                    let Ok(permit) = Arc::clone(&conn_slots).try_acquire_owned() else {
                        tracing::debug!(%peer, "DoT connection budget exhausted; dropping connection");
                        continue;
                    };
                    let acceptor = acceptors.acceptor();
                    let ctx = ctx.clone();
                    // Child token: cancelled when the accept loop stops
                    // (shutdown) or when this device's grant is revoked.
                    let conn_cancel = cancel.child_token();
                    tracker.spawn(
                        async move {
                            handle_connection(stream, peer, acceptor, ctx, conn_cancel).await;
                            drop(permit);
                        }
                        .instrument(tracing::Span::current()),
                    );
                }
                Err(e) => tracing::warn!(error = %e, "DoT accept failed"),
            }
        }
    }
}

/// Extract the grant-token label from an SNI: it must be exactly
/// `<single-label>.<fqdn>`. The bare apex (`sni == fqdn`), a name outside
/// the served domain, and a multi-label prefix all resolve to `None` —
/// the caller closes those connections.
pub(crate) fn token_from_sni<'a>(sni: &'a str, fqdn: &str) -> Option<&'a str> {
    let sni = sni.trim_end_matches('.');
    let prefix = sni.strip_suffix(fqdn)?;
    let label = prefix.strip_suffix('.')?;
    if label.is_empty() || label.contains('.') {
        return None;
    }
    Some(label)
}

/// Build a `REFUSED` response for a raw query, preserving its id and
/// question so the client correlates it. `None` if the bytes don't parse
/// (the caller then simply drops the frame).
fn build_refused(query: &[u8]) -> Option<Vec<u8>> {
    let request = Message::from_bytes(query).ok()?;
    let mut response = Message::response(request.metadata.id, OpCode::Query);
    response.metadata.recursion_desired = request.metadata.recursion_desired;
    response.metadata.recursion_available = true;
    response.metadata.response_code = ResponseCode::Refused;
    response.add_queries(request.queries.clone());
    response.to_bytes().ok()
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    acceptor: TlsAcceptor,
    ctx: ConnContext,
    conn_cancel: CancellationToken,
) {
    let Ok(Ok(tls_stream)) = tokio::time::timeout(HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await
    else {
        tracing::debug!(%peer, "DoT TLS handshake failed or timed out");
        return;
    };

    // SNI = authentication. Resolve the token label against the served
    // domain and the grant table; anything else is closed unanswered —
    // the closed-resolver contract. The token itself is a credential and
    // is never logged.
    let sni = tls_stream
        .get_ref()
        .1
        .server_name()
        .map(str::to_ascii_lowercase);
    let Some(sni) = sni else {
        tracing::debug!(%peer, "DoT connection without SNI; closing");
        return;
    };
    let Some(fqdn) = ctx.serving.canonical_fqdn() else {
        tracing::debug!(%peer, "DoT connection while unprovisioned; closing");
        return;
    };
    let Some(token) = token_from_sni(&sni, &fqdn.to_ascii_lowercase()) else {
        tracing::debug!(%peer, "DoT SNI is not a token hostname; closing");
        return;
    };

    // The listener is a background component: like the runners, it calls
    // the auth-gated service under the nil-admin context.
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };
    let grant =
        match auth_context::with_context(admin_ctx, ctx.private_dns.resolve_token(token)).await {
            Ok(Some(grant)) => grant,
            Ok(None) => {
                tracing::debug!(%peer, "DoT SNI token unknown or revoked; closing");
                return;
            }
            Err(e) => {
                tracing::warn!(%peer, error = %e, "DoT token resolution failed; closing");
                return;
            }
        };
    tracing::debug!(%peer, device_id = %grant.device_id, "DoT connection authenticated");

    // Register for revocation-driven teardown, so revoking this device's
    // grant cancels the connection instead of leaving it live until it
    // reconnects. Deregistered on exit.
    let conn_id = ctx.registry.register(grant.device_id, conn_cancel.clone());

    let identity = ClientIdentity::Device {
        device_id: grant.device_id,
        token: token.to_owned(),
    };
    serve_stream(tls_stream, peer, token, identity, &ctx, &conn_cancel).await;

    ctx.registry.deregister(grant.device_id, conn_id);
}

/// Drain finished response frames onto `write_half`, framed per RFC 7766.
/// Every write is bounded by `write_bound` and the task exits on
/// `cancel`, so a reader that stalls its receive window can't pin the
/// writer (and, through it, the connection and `stop()`).
async fn writer_task(
    mut write_half: tokio::io::WriteHalf<tokio_rustls::server::TlsStream<TcpStream>>,
    mut frame_rx: mpsc::Receiver<Vec<u8>>,
    write_bound: Duration,
    cancel: CancellationToken,
) {
    loop {
        let frame = tokio::select! {
            () = cancel.cancelled() => break,
            frame = frame_rx.recv() => match frame {
                Some(frame) => frame,
                None => break,
            },
        };
        let Ok(len) = u16::try_from(frame.len()) else {
            tracing::warn!("DoT response exceeds TCP frame bound; dropping");
            continue;
        };
        let write = async {
            write_half.write_all(&len.to_be_bytes()).await?;
            write_half.write_all(&frame).await
        };
        match tokio::time::timeout(write_bound, write).await {
            Ok(Ok(())) => {}
            // Write error, or the reader stalled past the bound — stop; the
            // connection is unusable.
            Ok(Err(_)) | Err(_) => break,
        }
    }
    let _ = write_half.shutdown().await;
}

/// Serve framed queries on an authenticated stream until EOF, idle
/// timeout, a protocol violation, or cancellation.
async fn serve_stream(
    tls_stream: tokio_rustls::server::TlsStream<TcpStream>,
    peer: SocketAddr,
    token: &str,
    identity: ClientIdentity,
    ctx: &ConnContext,
    cancel: &CancellationToken,
) {
    let (mut read_half, write_half) = tokio::io::split(tls_stream);

    // One writer task serialises responses onto the stream; query tasks
    // hand it finished frames, so pipelined queries may complete (and be
    // written) out of order.
    let (frame_tx, frame_rx) = mpsc::channel::<Vec<u8>>(MAX_DOT_CONNS);
    let writer = tokio::spawn(
        writer_task(write_half, frame_rx, ctx.idle_timeout, cancel.clone())
            .instrument(tracing::Span::current()),
    );

    // Bounds concurrent in-flight query tasks per connection: the read loop
    // parks on acquire when the budget is spent (backpressure) rather than
    // spawning without limit.
    let inflight = Arc::new(Semaphore::new(MAX_INFLIGHT_QUERIES_PER_CONN));
    let queries = TaskTracker::new();
    loop {
        // Backpressure: wait for an in-flight slot before reading the next
        // frame. Cancellation and a dead writer both unblock this.
        let permit = tokio::select! {
            () = cancel.cancelled() => break,
            permit = Arc::clone(&inflight).acquire_owned() => match permit {
                Ok(permit) => permit,
                Err(_) => break, // semaphore closed (never, but be safe)
            },
        };

        let mut len_buf = [0u8; 2];
        let read = tokio::select! {
            () = cancel.cancelled() => break,
            read = tokio::time::timeout(ctx.idle_timeout, read_half.read_exact(&mut len_buf)) => read,
        };
        match read {
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => break, // EOF / error / idle timeout
        }
        let len = usize::from(u16::from_be_bytes(len_buf));
        if len == 0 {
            tracing::debug!(%peer, "DoT zero-length frame; closing");
            break;
        }
        let mut msg = vec![0u8; len];
        match tokio::time::timeout(ctx.idle_timeout, read_half.read_exact(&mut msg)).await {
            Ok(Ok(_)) => {}
            Ok(Err(_)) | Err(_) => break,
        }

        // Per-token rate limit, read from the live DNS config each query so
        // a config change reaches this (long-lived) connection without a
        // reconnect. A breach is answered REFUSED, keeping the connection
        // (a reset would just make a fail-closed client reconnect-storm).
        let rate = ctx.pipeline.config.read().await.rate_limit_per_second;
        if !ctx
            .token_limiter
            .check_at(token.to_owned(), rate, Instant::now())
        {
            tracing::debug!(%peer, "DoT query exceeded per-token rate limit; refusing");
            if let Some(frame) = build_refused(&msg) {
                let _ = frame_tx.try_send(frame);
            }
            continue;
        }

        let pipeline = Arc::clone(&ctx.pipeline);
        let identity = identity.clone();
        let frame_tx = frame_tx.clone();
        let query_timeout = ctx.query_timeout;
        let query_cancel = cancel.clone();
        queries.spawn(
            async move {
                // Held for the task's lifetime, then released, freeing the
                // in-flight slot the read loop is gated on.
                let _permit = permit;
                let run = async {
                    let (capture, mut rx) = ReplyCapture::channel();
                    let reply: Arc<dyn DnsSocket> = Arc::new(capture);
                    if let Err(e) = pipeline
                        .handle(&msg, peer, &reply, identity, TransportProtocol::Dot)
                        .await
                    {
                        tracing::debug!(error = %e, "DoT query failed in pipeline");
                    }
                    drop(reply);
                    // The pipeline sends at most one frame — and possibly
                    // none (malformed packet), so the wait is bounded.
                    if let Ok(Some(frame)) = tokio::time::timeout(query_timeout, rx.recv()).await {
                        let _ = frame_tx.send(frame).await;
                    }
                };
                // Cancellation (shutdown / revocation) drops the query
                // promptly rather than waiting out query_timeout.
                tokio::select! {
                    () = query_cancel.cancelled() => {}
                    () = run => {}
                }
            }
            .instrument(tracing::Span::current()),
        );
    }

    // Wind the connection down: cancel so the writer and any in-flight
    // query tasks stop promptly, then drain and close.
    cancel.cancel();
    queries.close();
    queries.wait().await;
    drop(frame_tx);
    let _ = writer.await;
}
