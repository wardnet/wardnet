//! The transport-independent DNS resolve core (#911).
//!
//! [`QueryPipeline`] owns every piece of shared state a query needs —
//! config, resolver, recursor, rate limiter, cache, filter, routing and
//! device snapshots, authoritative view — and exposes
//! [`QueryPipeline::handle`]: raw query bytes in, at most one response
//! sent on the supplied [`DnsSocket`] (exactly one on every resolution
//! path; none for a malformed or question-less packet). The UDP listener
//! in `server.rs` drives it directly; stream transports capture the response via
//! [`ReplyCapture`](crate::dns::reply_capture::ReplyCapture) instead of
//! this being rewritten into a `-> Response` shape — the raw-byte relay
//! paths (`forward_via_tunnel` / `forward_via_conditional`) have no
//! parsed `Message` on their parse-failure branch, so keeping the
//! send-on-socket seam preserves the REFUSED/negative/rebinding
//! semantics exactly.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use arc_swap::ArcSwap;
use chrono::Utc;
use hickory_proto::op::{Message, OpCode, ResponseCode};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use hickory_resolver::Resolver;
use hickory_resolver::lookup::Lookup;
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::recursor::{Recursor, RecursorError};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use uuid::Uuid;
use wardnet_common::dns::{
    DnsConfig, DnsQueryResult, DnsRecordSource, DnsRecordType, DnsResolutionMode, FilterAction,
    UpstreamDns, UpstreamId,
};
use wardnet_common::net::is_private_ip;
use wardnetd_data::repository::{QueryLogRow, TunnelRepository};
use wardnetd_services::DnsFilterService;
use wardnetd_services::dns::DnsLogSink;
use wardnetd_services::dns::authoritative::{AuthoritativeView, parse_conditional_upstream};
use wardnetd_services::dns::cache::DnsCache;
use wardnetd_services::dns::server::DnsSocket;

use crate::dns::rate_limit::RateLimiter;

pub(crate) type TokioResolver = Resolver<TokioRuntimeProvider>;
pub(crate) type TokioRecursor = Recursor<TokioRuntimeProvider>;

/// Maximum number of pre-bound UDP sockets kept in the conditional-forwarding pool.
const CONDITIONAL_SOCKET_POOL_SIZE: usize = 8;

/// The daemon-owned local DNS zone. Single-label queries that miss the
/// authoritative view are retried under this suffix (the local search-domain
/// hop) — but only `System`-sourced records are adopted, so `wardnet` resolves
/// like `wardnet.lan` while device-chosen DHCP names stay bare-unreachable. Kept
/// in sync with the seeded `.lan` zone (`main.rs`, `dhcp_lan_runner`,
/// `20260414000000_dns.sql`).
const LOCAL_ZONE_SUFFIX: &str = "lan";

/// Who is asking. The UDP listener only ever knows the source address of
/// the datagram, so it attributes the query by IP through the device
/// snapshot — today's behaviour for every UDP/LAN client. A stream
/// transport that authenticates the client out of band (the DNS-over-TLS
/// listener attributes the connection by its SNI device token) hands the
/// pipeline the resolved device directly, so log rows and stats don't
/// depend on an IP → device mapping that may not cover the client's
/// address.
///
/// Scope: the identity feeds **attribution only** — the `device_id`
/// stamped on log rows and stats. Rate limiting, per-client upstream
/// selection, and the filter-policy lookup all still key on the
/// transport-level source IP (see [`QueryPipeline::handle`]). A
/// transport whose peer addresses the snapshots and filter contexts
/// don't cover gets the default upstream and default filter policy for
/// those clients; extending the IP-keyed paths to honour the device
/// identity is on the transport that needs it, not silently implied by
/// this enum.
#[derive(Clone)]
pub enum ClientIdentity {
    /// Attribute by source IP via the device snapshot.
    Ip(IpAddr),
    /// The transport already authenticated a specific device (e.g. via
    /// the per-device token presented in the TLS SNI). The token rides
    /// along for the listener that validated it; the pipeline itself
    /// reads only `device_id`.
    Device { device_id: Uuid, token: String },
}

// Manual impl so the SNI device token — a per-device credential — can't
// end up in the log file via a stray `tracing::debug!(?client)`. Same
// treatment as the WireGuard key redaction (#954).
impl std::fmt::Debug for ClientIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ip(ip) => f.debug_tuple("Ip").field(ip).finish(),
            Self::Device {
                device_id,
                token: _,
            } => f
                .debug_struct("Device")
                .field("device_id", device_id)
                .field("token", &"[REDACTED]")
                .finish(),
        }
    }
}

/// Which transport delivered a query. Stamped on every log row (the
/// `protocol` column, issue #912) so `DoT`-served queries stay attributable
/// after the fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportProtocol {
    /// Classic cleartext UDP on `:53`.
    Udp,
    /// DNS-over-TLS on `:853` (RFC 7858).
    Dot,
}

impl TransportProtocol {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Udp => "udp",
            Self::Dot => "dot",
        }
    }
}

/// Write-time attribution stamped on every log row for one query: the
/// resolved device (if any) plus the transport it arrived over. `Copy`, so
/// the forwarding helpers can pass it along without threading two
/// parameters.
#[derive(Clone, Copy)]
pub(crate) struct QueryAttribution {
    pub(crate) device_id: Option<Uuid>,
    pub(crate) protocol: TransportProtocol,
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

/// The resolve core shared by every DNS transport.
///
/// Holds the per-query state that used to live directly on
/// `UdpDnsServer`; the server now owns an `Arc<QueryPipeline>` and the
/// listener loop calls [`QueryPipeline::handle`] per datagram. Fields are
/// `pub(crate)` so the server's config/lifecycle plumbing (`update_config`,
/// cache stats, the authoritative-view swap) keeps operating on the same
/// shared state the handlers read.
pub struct QueryPipeline {
    pub(crate) config: Arc<RwLock<DnsConfig>>,
    /// Shared upstream resolver. Lives behind a lock (rather than being
    /// per-query) so `update_config` can rebuild and swap it when
    /// upstream servers / DNSSEC / protocol change, without restarting
    /// the listener. Built from the current config; rebuilt on every
    /// `update_config`.
    pub(crate) resolver: Arc<RwLock<TokioResolver>>,
    /// Recursive resolver (Stage 5). `Some` only when
    /// `resolution_mode == Recursive`; rebuilt/cleared in `update_config`
    /// when the mode or the DNSSEC toggle changes, so forwarding mode
    /// (the default) carries no recursor state.
    pub(crate) recursor: Arc<RwLock<Option<TokioRecursor>>>,
    /// Per-client-IP token-bucket rate limiter (Stage 4). Enforced at the
    /// top of [`QueryPipeline::handle`]; disabled when
    /// `rate_limit_per_second == 0`.
    pub(crate) rate_limiter: Arc<RateLimiter>,
    pub(crate) cache: Arc<RwLock<DnsCache>>,
    pub(crate) dns_filter: Arc<dyn DnsFilterService>,
    pub(crate) log_sink: Option<Arc<DnsLogSink>>,
    /// Lock-free per-query upstream-selection snapshot, populated by the
    /// routing service. Maps a tunneled-device IP to `Tunnel(_)` only
    /// when that tunnel has `override_default_dns = true`.
    pub(crate) routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
    /// Lock-free IP → device-id snapshot, kept current by the
    /// device-snapshot listener. Consulted once per IP-attributed query
    /// so log rows and stats carry the device identity as of query time —
    /// re-deriving it at read time would misattribute traffic after DHCP
    /// hands the IP to a different device.
    pub(crate) device_snapshot: Arc<ArcSwap<HashMap<IpAddr, Uuid>>>,
    /// Tunnel repository — needed to translate `Tunnel(uuid)` from the
    /// routing snapshot into the interface name + DNS upstream we should
    /// forward to via `SO_BINDTODEVICE`.
    pub(crate) tunnel_repo: Arc<dyn TunnelRepository>,
    /// Cache of per-tunnel forwarders (interface + upstream addr). Keyed
    /// by tunnel UUID. Lazily populated on first miss; stale entries are
    /// fine — a flipped `override_default_dns` simply changes which
    /// upstream the snapshot points at, not the forwarder behaviour
    /// itself, and unused entries quietly age out at process restart.
    pub(crate) tunnel_forwarders: Arc<RwLock<HashMap<Uuid, Arc<TunnelForwarderInfo>>>>,
    /// Lock-free snapshot of enabled local authoritative records and
    /// conditional forwarding rules. Swapped atomically by the server's
    /// `update_authoritative_view` whenever `DnsLocalChanged` fires so
    /// each query reads a consistent view without taking a lock.
    pub(crate) authoritative_view: Arc<ArcSwap<AuthoritativeView>>,
    /// Pool of pre-bound UDP sockets for conditional forwarding. Re-using sockets
    /// avoids the per-query `bind` syscall. Each socket is `connect`-ed to the
    /// target upstream just before use, which also restricts the kernel to
    /// accepting responses only from that peer address (closing the off-path
    /// spoofing window). The pool is capped at [`CONDITIONAL_SOCKET_POOL_SIZE`];
    /// under-capacity sockets are created on demand.
    pub(crate) conditional_socket_pool: Arc<tokio::sync::Mutex<Vec<UdpSocket>>>,
}

impl QueryPipeline {
    /// Build a pipeline around the query-path state the server constructs.
    /// The tunnel-forwarder cache, the (initially empty) authoritative
    /// view, and the conditional socket pool are pipeline-internal, so
    /// they are created here rather than passed in.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        config: Arc<RwLock<DnsConfig>>,
        resolver: Arc<RwLock<TokioResolver>>,
        recursor: Arc<RwLock<Option<TokioRecursor>>>,
        rate_limiter: Arc<RateLimiter>,
        cache: Arc<RwLock<DnsCache>>,
        dns_filter: Arc<dyn DnsFilterService>,
        routing_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
        device_snapshot: Arc<ArcSwap<HashMap<IpAddr, Uuid>>>,
        tunnel_repo: Arc<dyn TunnelRepository>,
    ) -> Self {
        Self {
            config,
            resolver,
            recursor,
            rate_limiter,
            cache,
            dns_filter,
            log_sink: None,
            routing_snapshot,
            device_snapshot,
            tunnel_repo,
            tunnel_forwarders: Arc::new(RwLock::new(HashMap::new())),
            authoritative_view: Arc::new(ArcSwap::from_pointee(AuthoritativeView::empty())),
            conditional_socket_pool: Arc::new(tokio::sync::Mutex::new(Vec::new())),
        }
    }

    /// Handle one raw DNS query. Every resolution path sends exactly one
    /// response on `reply` — but there are two zero-send exits a
    /// transport must expect: a packet that fails to parse returns `Err`,
    /// and a parsed message with no question returns `Ok(())`, both
    /// without answering. A caller awaiting a captured response (see
    /// `reply_capture`) must bound the wait rather than rely on a frame
    /// always arriving.
    ///
    /// `src` is the transport-level peer address — it keys the rate
    /// limiter and the per-client upstream selection, and is stamped on
    /// log rows. `client` carries the attribution identity (see
    /// [`ClientIdentity`]); the UDP listener passes `Ip(src.ip())`, and a
    /// device-authenticated client's filter policy resolves through its
    /// device context rather than the per-IP map. `protocol` names the
    /// transport for the log row's `protocol` column.
    #[allow(clippy::too_many_lines)]
    pub async fn handle(
        &self,
        packet: &[u8],
        src: SocketAddr,
        reply: &Arc<dyn DnsSocket>,
        client: ClientIdentity,
        protocol: TransportProtocol,
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

        // Resolve the querying device once. IP-attributed clients go through
        // the point-in-time IP → device-id snapshot (wait-free load, no
        // locks); transports that authenticate the device out of band carry
        // the identity directly. Carried into every recorded log row and stat
        // so the attribution survives DHCP later reassigning this IP to a
        // different device. `None` = unknown client.
        let device_id = match &client {
            ClientIdentity::Ip(ip) => self.device_snapshot.load().get(ip).copied(),
            ClientIdentity::Device { device_id, .. } => Some(*device_id),
        };
        let attribution = QueryAttribution {
            device_id,
            protocol,
        };
        let log_sink = self.log_sink.as_deref();

        // 1. Rate limit (per-client token bucket). 0 disables; shed flooding
        // clients before any resolution work, returning REFUSED. The rate is
        // read lock-free from the limiter (no config lock on the hot path).
        if !self.rate_limiter.check(src.ip()) {
            send_refused(reply, src, id, &request).await?;
            record_query(
                log_sink,
                &domain,
                rtype,
                src,
                attribution,
                DnsQueryResult::RateLimited.as_str(),
                None,
                start.elapsed(),
            );
            tracing::debug!(%domain, %src, "rate-limited DNS query");
            return Ok(());
        }

        // 0. Resolve upstream pool for this client.
        let upstream_id = self
            .routing_snapshot
            .load()
            .get(&src.ip())
            .copied()
            .unwrap_or(UpstreamId::Default);

        let domain_lower = domain.trim_end_matches('.').to_ascii_lowercase();
        let view = self.authoritative_view.load();

        // 0.5. Authoritative answer — BEFORE cache, bypasses filter.
        let is_any = rtype == hickory_proto::rr::RecordType::ANY;
        let our_rtype = rtype_from_hickory(rtype);

        let lookup = |name: &str| {
            if is_any {
                view.lookup_all(name)
            } else if let Some(rt) = our_rtype {
                view.lookup(name, rt)
            } else {
                None
            }
        };

        let mut auth_lookup = lookup(&domain_lower);

        // Local search-domain hop: a single-label query (`wardnet`) that misses the
        // authoritative view is retried under the local `.lan` zone, so the bare name
        // resolves to its `<label>.lan` record without the client needing a search
        // domain configured — we control the resolver, so we do the hop ourselves. The
        // answer keeps the queried bare name (a direct A/AAAA answer), and a miss falls
        // through to normal forwarding — we never synthesize NXDOMAIN/NODATA for it.
        //
        // Restricted to `System`-sourced records (the daemon-seeded self-name). DHCP
        // `.lan` records carry a device-chosen hostname (option 12), so adopting them
        // here would let a device make itself resolvable at a bare single label the
        // client never had a search domain for — e.g. a device claiming hostname
        // `wpad` would answer a bare `wpad` lookup that previously forwarded upstream.
        // Those names stay reachable at their explicit `<name>.lan`, just not bare.
        if auth_lookup.is_none_or(<[_]>::is_empty)
            && !domain_lower.is_empty()
            && !domain_lower.contains('.')
        {
            let local = format!("{domain_lower}.{LOCAL_ZONE_SUFFIX}");
            if let Some(recs) = lookup(&local)
                && !recs.is_empty()
                && recs.iter().all(|r| r.source == DnsRecordSource::System)
            {
                auth_lookup = Some(recs);
            }
        }

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
                            if let Ok(rdata) =
                                make_rdata(t_rec.record_type, &t_rec.value, &target_name)
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
            reply.send_to(&bytes, src).await?;
            tracing::trace!(%domain, ?rtype, "authoritative answer");
            record_query(
                log_sink,
                &domain,
                rtype,
                src,
                attribution,
                DnsQueryResult::Authoritative.as_str(),
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
                            "malformed conditional forwarding upstream - falling through to default"
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
        let authoritative_negative: Option<(Option<Record>, bool)> =
            if conditional_upstream.is_none() {
                view.authoritative_zone(&domain_lower).map(|z| {
                    let name_exists =
                        view.lookup_all(&domain_lower).is_some() || domain_lower == z.name;
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
            reply.send_to(&bytes, src).await?;
            let result = if name_exists {
                DnsQueryResult::AuthoritativeNodata.as_str()
            } else {
                DnsQueryResult::AuthoritativeNxdomain.as_str()
            };
            tracing::trace!(%domain, ?rtype, result, "authoritative negative answer: {result}");
            record_query(
                log_sink,
                &domain,
                rtype,
                src,
                attribution,
                result,
                None,
                start.elapsed(),
            );
            return Ok(());
        }

        // 1. Cache, keyed by upstream so a tunneled device's answer doesn't
        //    bleed into a LAN device's lookup of the same domain (or vice
        //    versa).
        let cached = {
            let cache_guard = self.cache.read().await;
            cache_guard.get(upstream_id, &domain, rtype)
        };
        if let Some(mut response) = cached {
            response.metadata.id = id;
            let bytes = response.to_bytes()?;
            reply.send_to(&bytes, src).await?;
            tracing::trace!(%domain, ?rtype, ?upstream_id, "cache hit");
            record_query(
                log_sink,
                &domain,
                rtype,
                src,
                attribution,
                DnsQueryResult::CacheHit.as_str(),
                None,
                start.elapsed(),
            );
            return Ok(());
        }

        // 2. Filter. An IP-attributed client resolves its policy through the
        //    per-IP map; a device-authenticated client (DoT SNI token) gets
        //    its own device-keyed context, so its profiles apply even when
        //    `src` is an address the per-IP map doesn't cover.
        let outcome = match &client {
            ClientIdentity::Ip(_) => self.dns_filter.check(&domain_lower, rtype, src.ip()).await,
            ClientIdentity::Device { device_id, .. } => {
                self.dns_filter
                    .check_for_device(&domain_lower, rtype, *device_id, src.ip())
                    .await
            }
        };

        match outcome.action {
            FilterAction::Block => {
                let mut response = Message::response(id, OpCode::Query);
                response.metadata.recursion_desired = true;
                response.metadata.recursion_available = true;
                response.metadata.response_code = ResponseCode::NXDomain;
                response.add_queries(request.queries.clone());
                let bytes = response.to_bytes()?;
                reply.send_to(&bytes, src).await?;
                tracing::trace!(%domain, ?rtype, "blocked by filter");
                record_query(
                    log_sink,
                    &domain,
                    rtype,
                    src,
                    attribution,
                    DnsQueryResult::Blocked.as_str(),
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
                reply.send_to(&bytes, src).await?;
                tracing::trace!(%domain, ?rtype, %ip, "rewritten by filter");
                record_query(
                    log_sink,
                    &domain,
                    rtype,
                    src,
                    attribution,
                    DnsQueryResult::Rewritten.as_str(),
                    None,
                    start.elapsed(),
                );
                return Ok(());
            }
            FilterAction::Pass => {}
        }

        // 3. Forward to upstream — conditional forwarding overrides upstream_id.
        let pass_result = if outcome.would_have_blocked {
            DnsQueryResult::BlockedSkipped.as_str()
        } else {
            DnsQueryResult::Forwarded.as_str()
        };

        if let Some(cond_upstream) = conditional_upstream {
            if let Err(e) = forward_via_conditional(
                cond_upstream,
                reply,
                &self.cache,
                &self.config,
                log_sink,
                packet,
                &request,
                id,
                src,
                attribution,
                &domain,
                rtype,
                start,
                pass_result,
                upstream_id,
                &self.conditional_socket_pool,
            )
            .await
            {
                tracing::warn!(
                    error = %e,
                    upstream = %cond_upstream,
                    %domain,
                    "conditional DNS forward failed; returning ServFail"
                );
                send_servfail(reply, src, id, &request).await?;
                record_query(
                    log_sink,
                    &domain,
                    rtype,
                    src,
                    attribution,
                    DnsQueryResult::UpstreamError.as_str(),
                    Some(cond_upstream.ip().to_string()),
                    start.elapsed(),
                );
            }
            return Ok(());
        }

        match upstream_id {
            UpstreamId::Default => {
                let recursive =
                    self.config.read().await.resolution_mode == DnsResolutionMode::Recursive;
                if recursive {
                    resolve_via_recursor(
                        &self.recursor,
                        &self.resolver,
                        reply,
                        &self.config,
                        &self.cache,
                        log_sink,
                        request,
                        id,
                        src,
                        attribution,
                        &domain,
                        rtype,
                        start,
                        pass_result,
                        upstream_id,
                    )
                    .await?;
                } else {
                    forward_via_default_resolver(
                        &self.resolver,
                        reply,
                        &self.config,
                        &self.cache,
                        log_sink,
                        request,
                        id,
                        src,
                        attribution,
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
                match get_or_build_tunnel_forwarder(
                    &self.tunnel_forwarders,
                    &self.tunnel_repo,
                    tunnel_id,
                )
                .await
                {
                    Ok(forwarder) => {
                        if let Err(e) = forward_via_tunnel(
                            &forwarder,
                            reply,
                            &self.cache,
                            &self.config,
                            log_sink,
                            packet,
                            &request,
                            id,
                            src,
                            attribution,
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
                            send_servfail(reply, src, id, &request).await?;
                            record_query(
                                log_sink,
                                &domain,
                                rtype,
                                src,
                                attribution,
                                DnsQueryResult::UpstreamError.as_str(),
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
                        send_servfail(reply, src, id, &request).await?;
                        record_query(
                            log_sink,
                            &domain,
                            rtype,
                            src,
                            attribution,
                            DnsQueryResult::UpstreamError.as_str(),
                            None,
                            start.elapsed(),
                        );
                    }
                }
            }
        }

        Ok(())
    }
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
    attribution: QueryAttribution,
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
                attribution,
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
        // The upstream answered "no such record / no such name" — a valid
        // negative resolution, not an upstream failure. Relay it as
        // NODATA/NXDOMAIN rather than SERVFAIL.
        Err(e) if e.is_no_records_found() || e.is_nx_domain() => {
            let nx_domain = e.is_nx_domain();
            relay_negative(
                socket,
                cache,
                log_sink,
                &request,
                id,
                src,
                attribution,
                domain,
                rtype,
                start,
                pass_result,
                upstream_id,
                upstream,
                nx_domain,
                e.into_soa(),
                cfg.cache_ttl_min_secs,
                cfg.cache_ttl_max_secs,
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
                attribution,
                DnsQueryResult::UpstreamError.as_str(),
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
    attribution: QueryAttribution,
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
        send_negative(socket, src, id, request, false, None).await?;
        record_query(
            log_sink,
            domain,
            rtype,
            src,
            attribution,
            DnsQueryResult::RebindingBlocked.as_str(),
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
        attribution,
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
    attribution: QueryAttribution,
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

    let dnssec_ok = config.read().await.dnssec_enabled;

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

    handle_recursor_outcome(
        recursor_result,
        resolver,
        socket,
        config,
        cache,
        log_sink,
        request,
        id,
        src,
        attribution,
        domain,
        rtype,
        start,
        pass_result,
        upstream_id,
    )
    .await
}

/// React to the recursor's outcome: relay a resolved answer, fall back to
/// forwarding on genuine failure, or SERVFAIL when neither is possible.
/// Split from `resolve_via_recursor` so tests can inject outcomes (the
/// recursor itself always walks from the real root servers).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_recursor_outcome(
    recursor_result: Option<Result<Message, RecursorError>>,
    resolver: &Arc<RwLock<TokioResolver>>,
    socket: &Arc<dyn DnsSocket>,
    config: &Arc<RwLock<DnsConfig>>,
    cache: &Arc<RwLock<DnsCache>>,
    log_sink: Option<&DnsLogSink>,
    request: Message,
    id: u16,
    src: SocketAddr,
    attribution: QueryAttribution,
    domain: &str,
    rtype: hickory_proto::rr::RecordType,
    start: std::time::Instant,
    pass_result: &str,
    upstream_id: UpstreamId,
) -> anyhow::Result<()> {
    let (has_upstreams, rebinding, ttl_min, ttl_max) = {
        let cfg = config.read().await;
        (
            !cfg.upstream_servers.is_empty(),
            cfg.rebinding_protection,
            cfg.cache_ttl_min_secs,
            cfg.cache_ttl_max_secs,
        )
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
                attribution,
                domain,
                rtype,
                start,
                pass_result,
                upstream_id,
                Some(DnsQueryResult::Recursive.as_str().to_owned()),
                rebinding,
                ttl_min,
                ttl_max,
                &msg.answers,
            )
            .await?;
        }
        // A negative answer (NODATA / NXDOMAIN) is a valid resolution, not a
        // failure: relay it instead of falling back to the forwarder, which
        // would re-ask a question the authority already answered — and, when
        // the forwarder agreed the record doesn't exist, SERVFAIL the client.
        Some(Err(e)) if e.is_no_records_found() || e.is_nx_domain() => {
            let nx_domain = e.is_nx_domain();
            relay_negative(
                socket,
                cache,
                log_sink,
                &request,
                id,
                src,
                attribution,
                domain,
                rtype,
                start,
                pass_result,
                upstream_id,
                Some(DnsQueryResult::Recursive.as_str().to_owned()),
                nx_domain,
                e.into_soa(),
                ttl_min,
                ttl_max,
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
                    attribution,
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
                    attribution,
                    DnsQueryResult::RecursorFailed.as_str(),
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
    attribution: QueryAttribution,
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

    let mut result = pass_result;
    if let Ok(parsed) = Message::from_bytes(&buf) {
        let (is_negative, raw_ttl) = classify_response(&parsed);
        result = relayed_result(is_negative, pass_result);
        if raw_ttl > 0 {
            let cfg = config.read().await;
            let mut cache_guard = cache.write().await;
            cache_guard.insert(
                upstream_id,
                domain,
                rtype,
                parsed,
                raw_ttl,
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
        attribution,
        result,
        Some(forwarder.upstream.ip().to_string()),
        elapsed,
    );
    Ok(())
}

/// Classify a parsed upstream response for caching: positive answers use the
/// answers' minimum TTL; negative answers (NXDOMAIN, or NOERROR with zero
/// answers) use the RFC 2308 negative TTL — min(SOA record TTL, SOA MINIMUM)
/// from the authority section, or 0 (uncacheable) when the zone forbids
/// negative caching or carries no SOA. Returns `(is_negative, raw_ttl)`.
fn classify_response(parsed: &Message) -> (bool, u32) {
    use hickory_proto::rr::RData;

    let negative = parsed.metadata.response_code == ResponseCode::NXDomain
        || (parsed.metadata.response_code == ResponseCode::NoError && parsed.answers.is_empty());
    if negative {
        let ttl = parsed
            .authorities
            .iter()
            .find_map(|r| match &r.data {
                RData::SOA(soa) => Some(r.ttl.min(soa.minimum)),
                _ => None,
            })
            .unwrap_or(0);
        return (true, ttl);
    }
    let min_ttl = parsed.answers.iter().map(|r| r.ttl).min().unwrap_or(0);
    (false, min_ttl)
}

/// The query-log result for a relayed upstream response: negative answers
/// log as `"negative"`, except that the filter dry-run marker
/// (`blocked_skipped`) outranks it for blocklist evaluation.
fn relayed_result(is_negative: bool, pass_result: &str) -> &str {
    if is_negative && pass_result != DnsQueryResult::BlockedSkipped.as_str() {
        DnsQueryResult::Negative.as_str()
    } else {
        pass_result
    }
}

/// Send a negative response — NODATA (empty NOERROR) or NXDOMAIN — with an
/// optional pre-stamped authority record (the zone's SOA) so clients can
/// negatively cache (RFC 2308). A negative answer is a *successful*
/// resolution: "the name (or record type) does not exist" must never surface
/// as SERVFAIL. Returns the response so callers can cache it.
async fn send_negative(
    socket: &Arc<dyn DnsSocket>,
    src: SocketAddr,
    id: u16,
    request: &Message,
    nx_domain: bool,
    authority: Option<hickory_proto::rr::Record>,
) -> anyhow::Result<Message> {
    let mut response = Message::response(id, OpCode::Query);
    response.metadata.recursion_desired = true;
    response.metadata.recursion_available = true;
    response.metadata.response_code = if nx_domain {
        ResponseCode::NXDomain
    } else {
        ResponseCode::NoError
    };
    response.add_queries(request.queries.clone());
    if let Some(authority) = authority {
        response.add_authority(authority);
    }
    let bytes = response.to_bytes()?;
    socket.send_to(&bytes, src).await?;
    Ok(response)
}

/// Shared negative-answer responder for the recursive and forwarding paths:
/// stamps the RFC 2308 negative TTL (min(SOA record TTL, SOA MINIMUM),
/// clamped to the admin's cache bounds) on the relayed SOA, sends the
/// response, caches it so repeated negative queries are served locally, and
/// records the query as `"negative"` — unless the filter dry-run marker
/// (`blocked_skipped`) applies, which outranks it for blocklist evaluation.
#[allow(clippy::too_many_arguments)]
async fn relay_negative(
    socket: &Arc<dyn DnsSocket>,
    cache: &Arc<RwLock<DnsCache>>,
    log_sink: Option<&DnsLogSink>,
    request: &Message,
    id: u16,
    src: SocketAddr,
    attribution: QueryAttribution,
    domain: &str,
    rtype: hickory_proto::rr::RecordType,
    start: std::time::Instant,
    pass_result: &str,
    upstream_id: UpstreamId,
    upstream_label: Option<String>,
    nx_domain: bool,
    soa: Option<Box<hickory_proto::rr::Record<hickory_proto::rr::rdata::SOA>>>,
    cache_ttl_min_secs: u32,
    cache_ttl_max_secs: u32,
) -> anyhow::Result<()> {
    tracing::debug!(
        %domain,
        ?rtype,
        nx_domain,
        "negative answer for {domain} ({rtype:?}): nx_domain={nx_domain}"
    );

    // RFC 2308 §5: the negative TTL is min(SOA record TTL, SOA MINIMUM). A
    // raw value of 0 means the zone forbids negative caching — honour it
    // (like the positive path's raw-TTL gate) rather than raising it to the
    // admin's cache floor. Non-zero values get the same `.max(min).min(max)`
    // clamp DnsCache::insert applies, so the TTL stamped on the wire and the
    // daemon's own cache lifetime always agree.
    let (authority, negative_ttl) = if let Some(soa) = soa {
        let soa = *soa;
        let raw_ttl = soa.ttl.min(soa.data.minimum);
        let ttl = if raw_ttl == 0 {
            0
        } else {
            raw_ttl.max(cache_ttl_min_secs).min(cache_ttl_max_secs)
        };
        let mut record = soa.into_record_of_rdata();
        record.ttl = ttl;
        (Some(record), ttl)
    } else {
        (None, 0)
    };

    let response = send_negative(socket, src, id, request, nx_domain, authority).await?;

    if negative_ttl > 0 {
        let mut cache_guard = cache.write().await;
        cache_guard.insert(
            upstream_id,
            domain,
            rtype,
            response,
            negative_ttl,
            cache_ttl_min_secs,
            cache_ttl_max_secs,
        );
    }

    let result = relayed_result(true, pass_result);
    record_query(
        log_sink,
        domain,
        rtype,
        src,
        attribution,
        result,
        upstream_label,
        start.elapsed(),
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn record_query(
    sink: Option<&DnsLogSink>,
    domain: &str,
    rtype: hickory_proto::rr::RecordType,
    src: SocketAddr,
    attribution: QueryAttribution,
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
        device_id: attribution.device_id.map(|id| id.to_string()),
        protocol: attribution.protocol.as_str().to_owned(),
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
    attribution: QueryAttribution,
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
    let mut result = pass_result;
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

        let (is_negative, raw_ttl) = classify_response(&parsed);
        result = relayed_result(is_negative, pass_result);
        if raw_ttl > 0 {
            let cfg = config.read().await;
            let mut cache_guard = cache.write().await;
            cache_guard.insert(
                upstream_id,
                domain,
                rtype,
                parsed,
                raw_ttl,
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
        attribution,
        result,
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
