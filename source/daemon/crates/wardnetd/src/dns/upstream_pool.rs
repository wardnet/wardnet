//! The forwarding ladder (#1199): every configured upstream paired with its
//! own single-server resolver, plus the order the forwarder tries them in.
//!
//! ## Why we drive failover ourselves
//!
//! The obvious implementation is one hickory resolver holding every upstream,
//! steered by [`ServerOrderingStrategy`](hickory_resolver::config::ServerOrderingStrategy).
//! That is what this replaced, and it did not do what the admin UI says it
//! does. `NameServerPool::try_send` applies the ordering strategy as a *sort*
//! and then races `num_concurrent_reqs` servers — default **2** — in parallel,
//! returning whichever answers first and penalising the loser's SRTT. So
//! "Failover (in order)", which the UI describes as "queries go to the first
//! server; Wardnet only falls back to the next if it stops responding",
//! actually sent every query to the first *two* providers at once. For a
//! privacy gateway that is a promise broken twice over: the ordering is not
//! honoured, and a second provider sees traffic the admin never agreed to
//! show it.
//!
//! It also made the query log unfixable. hickory's `Lookup` carries no record
//! of which name server answered, so with a racing pool there is no honest
//! value for `dns_query_log.upstream` — which is why it used to report
//! `upstream_servers[0]` for every query regardless of what happened.
//!
//! Giving each upstream its own single-server resolver and walking them here
//! fixes all of it at once: the order is exactly what the admin configured,
//! only one provider is asked at a time, and the rung that answered is known
//! by construction, so the log names it exactly.
//!
//! The one thing we give up is hickory's SRTT-based ordering for "Fastest".
//! We replace it with the latency prober's EWMA — which is strictly better
//! for being the same number the admin is already looking at on the DNS page,
//! rather than a hidden statistic that could disagree with it.

use std::sync::Arc;

use hickory_resolver::config::{
    ConnectionConfig, NameServerConfig, ResolveHosts, ResolverConfig, ResolverOpts,
};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use wardnet_common::dns::{DnsConfig, DnsProtocol, ForwarderSelectionMode, UpstreamDns};
use wardnetd_services::dns::UpstreamHealth;

use crate::dns::pipeline::TokioResolver;

/// Last-resort upstream when the configured pool yields nothing usable.
/// Matches the address the pre-#1199 `build_resolver` fell back to, so a
/// misconfigured box behaves as it always has.
const FALLBACK_UPSTREAM: &str = "1.1.1.1";

/// One upstream and the resolver that talks to it, and only to it.
pub(crate) struct UpstreamEntry {
    pub(crate) upstream: UpstreamDns,
    pub(crate) resolver: TokioResolver,
}

impl UpstreamEntry {
    /// The address used as the query-log label and as the health-snapshot key.
    pub(crate) fn address(&self) -> &str {
        &self.upstream.address
    }
}

/// Every upstream the forwarding path may use, plus the ordered subset it
/// should actually try.
///
/// `all` is rebuilt only when the config changes; `serving` is recomputed
/// after every probe round. Keeping both means eviction and restoration never
/// tear down a resolver — the entries are shared `Arc`s, so an upstream that
/// drops out of `serving` and comes back keeps its warm `DoT`/`DoH`
/// connection.
pub(crate) struct UpstreamPool {
    /// Every usable upstream, evicted ones included — an entry has to survive
    /// eviction for anything to restore it when the prober sees it recover.
    /// Not necessarily one per configured upstream: unusable entries are
    /// dropped and a pool that would otherwise be empty gets a fallback (see
    /// `resolvable_upstreams`).
    all: Vec<Arc<UpstreamEntry>>,
    /// What the forwarder tries, in order. A subset of `all`, minus upstreams
    /// the prober reports unreachable, ordered per the forwarder mode.
    serving: Vec<Arc<UpstreamEntry>>,
}

impl UpstreamPool {
    /// Build the pool from config, with every upstream serving.
    ///
    /// Reachability is deliberately not consulted here: a config change is a
    /// fresh start, and an upstream the admin has just (re)configured deserves
    /// a chance to answer before we act on what we knew about the old one. The
    /// next probe round re-narrows `serving` within
    /// [`LATENCY_PROBE_INTERVAL`](crate::dns::server::LATENCY_PROBE_INTERVAL).
    pub(crate) fn build(config: &DnsConfig) -> Self {
        let all: Vec<Arc<UpstreamEntry>> = resolvable_upstreams(&config.upstream_servers)
            .into_iter()
            .map(|upstream| {
                let resolver = build_resolver(&upstream, config);
                Arc::new(UpstreamEntry { upstream, resolver })
            })
            .collect();

        let serving = effective(&all, config);
        Self { all, serving }
    }

    /// The upstreams to try, in order.
    pub(crate) fn serving(&self) -> &[Arc<UpstreamEntry>] {
        &self.serving
    }

    /// The same pool with `serving` recomputed against current reachability.
    ///
    /// Returns a new value rather than mutating, so the caller can publish it
    /// through an `ArcSwap` compare-and-swap and lose the race safely to a
    /// concurrent config rebuild.
    pub(crate) fn with_serving(&self, config: &DnsConfig, health: &UpstreamHealth) -> Self {
        Self {
            all: self.all.clone(),
            serving: serving_order(&self.all, config, health),
        }
    }
}

/// Narrow `all` to the upstreams the current forwarder mode allows, before
/// reachability is considered.
///
/// Only `Single` narrows: it pins one server and the others are never used.
/// A pinned address that is not in the pool is a config inconsistency the API
/// rejects, but if one slips through we fall back to the full pool rather than
/// serving nothing.
fn effective(all: &[Arc<UpstreamEntry>], config: &DnsConfig) -> Vec<Arc<UpstreamEntry>> {
    match (
        config.forwarder_selection_mode,
        config.single_upstream.as_deref(),
    ) {
        (ForwarderSelectionMode::Single, Some(addr)) => {
            let selected: Vec<_> = all
                .iter()
                .filter(|e| e.address() == addr)
                .map(Arc::clone)
                .collect();
            if selected.is_empty() {
                tracing::warn!(
                    single_upstream = %addr,
                    "selected upstream not found in the configured pool; falling back to the full pool"
                );
                all.to_vec()
            } else {
                selected
            }
        }
        _ => all.to_vec(),
    }
}

/// The ordered set of upstreams the forwarder should try.
///
/// Pure, so the eviction and ordering rules are unit-testable without a
/// network or a running prober.
///
/// - Upstreams the prober reports unreachable are dropped. An upstream it has
///   *not measured* is kept: absent from the snapshot means "no sample yet",
///   not "down", and treating the two alike would empty the pool on every
///   startup.
/// - If that leaves nothing, the full effective set is restored. A pool of
///   zero servers answers nothing, so when every upstream looks dead we are
///   better off asking them anyway — the prober may be wrong, and the
///   per-upstream deadline bounds the cost of finding out.
/// - `Fastest` orders by the prober's EWMA, unmeasured upstreams last. The
///   sort is stable, so ties and unmeasured entries keep the admin's
///   configured order.
pub(crate) fn serving_order(
    all: &[Arc<UpstreamEntry>],
    config: &DnsConfig,
    health: &UpstreamHealth,
) -> Vec<Arc<UpstreamEntry>> {
    let effective = effective(all, config);

    let mut serving: Vec<Arc<UpstreamEntry>> = effective
        .iter()
        .filter(|e| !health.is_unreachable(e.address()))
        .map(Arc::clone)
        .collect();

    if serving.is_empty() {
        serving = effective;
    }

    if config.forwarder_selection_mode == ForwarderSelectionMode::Fastest {
        serving.sort_by(|a, b| {
            let key = |e: &Arc<UpstreamEntry>| health.latency_ms(e.address()).unwrap_or(f64::MAX);
            key(a)
                .partial_cmp(&key(b))
                // No NaN reaches here (the EWMA folds finite samples only),
                // but a total order is required and silently reordering on a
                // surprise NaN beats a panic in the query path.
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    serving
}

/// Drop upstreams that cannot be turned into a resolver, and guarantee at
/// least one.
///
/// The address must be an IP literal — we never resolve an upstream's
/// hostname, since doing so would need the very resolver we are building.
/// Encrypted upstreams additionally need an SNI for certificate validation;
/// the API rejects those without one, and if a bad config slips through we
/// drop the upstream rather than silently downgrading it to plaintext.
///
/// The Cloudflare backstop preserves long-standing behaviour: a box with no
/// usable upstream still resolves rather than going dark. It is loud about it,
/// because it is a fallback nobody chose.
fn resolvable_upstreams(configured: &[UpstreamDns]) -> Vec<UpstreamDns> {
    let usable: Vec<UpstreamDns> = configured
        .iter()
        .filter(|u| {
            if u.address.parse::<std::net::IpAddr>().is_err() {
                tracing::warn!(address = %u.address, "skipping upstream: not a valid IP");
                return false;
            }
            if matches!(u.protocol, DnsProtocol::Tls | DnsProtocol::Https)
                && u.tls_server_name.is_none()
            {
                tracing::error!(
                    address = %u.address,
                    protocol = ?u.protocol,
                    "skipping encrypted upstream: tls_server_name is required for DoT/DoH",
                );
                return false;
            }
            true
        })
        .cloned()
        .collect();

    if usable.is_empty() {
        tracing::warn!("no valid upstream DNS servers, falling back to Cloudflare 1.1.1.1");
        return vec![UpstreamDns {
            address: FALLBACK_UPSTREAM.to_owned(),
            name: "Cloudflare".to_owned(),
            protocol: DnsProtocol::Udp,
            port: None,
            tls_server_name: None,
        }];
    }
    usable
}

/// The ladder's resolver for one upstream, timed from the current config.
fn build_resolver(upstream: &UpstreamDns, config: &DnsConfig) -> TokioResolver {
    resolver_for(
        upstream,
        config.dnssec_enabled,
        std::time::Duration::from_millis(u64::from(config.upstream_timeout_ms)),
    )
}

/// Build a resolver that talks to exactly one upstream.
///
/// Shared by the forwarding ladder and the latency prober so there is one
/// place that knows how to turn an [`UpstreamDns`] into a connection. They
/// each get their own *instance*, deliberately: for `DoT`/`DoH` a resolver
/// holds a persistent multiplexed connection, and probing over the connection
/// serving live queries would measure latency-plus-queueing rather than the
/// round-trip time the UI claims to be showing.
///
/// Every timing knob is set explicitly. Inheriting hickory's defaults is what
/// made a degraded upstream stall a query for 20-30s (#1199): `timeout`
/// defaults to 5s and `attempts` to 2 *retries*, and `RetryDnsHandle` reruns
/// the whole server ladder for each one.
///
/// - `timeout` bounds both this server's connection and hickory's own pool
///   loop, so one rung of the ladder cannot outlast it.
/// - `attempts = 0` disables hickory's retry: moving to the next upstream is
///   our retry, and it reaches a *different* server, which is the useful kind.
///   This is not "one packet" — `UdpClientStream` still retransmits up to four
///   datagrams spaced by `max(1.2 x SRTT, 333ms)` underneath.
/// - `num_concurrent_reqs = 1` is redundant while each resolver holds a single
///   server, and set anyway so that adding a second one here can never
///   silently reintroduce the parallel racing this module exists to avoid.
pub(crate) fn resolver_for(
    upstream: &UpstreamDns,
    dnssec_enabled: bool,
    timeout: std::time::Duration,
) -> TokioResolver {
    let mut resolver_config = ResolverConfig::default();

    let mut conn = match upstream.protocol {
        DnsProtocol::Udp => ConnectionConfig::udp(),
        DnsProtocol::Tcp => ConnectionConfig::tcp(),
        DnsProtocol::Tls | DnsProtocol::Https => {
            // `resolvable_upstreams` has already dropped encrypted upstreams
            // without an SNI, so this is infallible in practice.
            let sni: Arc<str> = Arc::from(upstream.tls_server_name.clone().unwrap_or_default());
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
        resolver_config.add_name_server(NameServerConfig::new(ip, true, vec![conn]));
    }

    let mut opts = ResolverOpts::default();
    // Answers are cached by our own `DnsCache`, which the filter and local-DNS
    // rebuilds know how to invalidate; a second cache inside hickory would
    // serve past those.
    opts.cache_size = 0;
    opts.use_hosts_file = ResolveHosts::Never;
    opts.timeout = timeout;
    opts.attempts = 0;
    opts.num_concurrent_reqs = 1;
    // DNS Stage 4 — local DNSSEC validation (opt-in; default off). hickory
    // validates signatures via the upstream as forwarder and surfaces bogus
    // responses as resolution errors, which the ladder treats as a failure of
    // that upstream and moves on: another server may serve a correctly-signed
    // answer for the same name.
    opts.validate = dnssec_enabled;

    TokioResolver::builder_with_config(resolver_config, TokioRuntimeProvider::default())
        .with_options(opts)
        .build()
        .expect("failed to build DNS resolver")
}
