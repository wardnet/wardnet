//! Production [`FirewallManager`] backed by pure netlink via the [`rustables`]
//! crate — no `nft` CLI (issue #307).
//!
//! `rustables` is synchronous: each operation builds a [`Batch`] of nftables
//! netlink messages and sends it to the kernel in one atomic round-trip, or
//! lists a chain's rules over a fresh socket. The manager holds no socket
//! state, so the blocking calls are offloaded to the Tokio blocking pool via
//! [`tokio::task::spawn_blocking`].
//!
//! Requires `CAP_NET_ADMIN`. The `nf_tables` kernel module must be loadable;
//! the `nft` userspace tool is no longer needed.
//!
//! Rule identity: every wardnet-managed rule carries a comment encoded in the
//! nftables comment UDATA TLV (see [`comment_udata`]). Removal lists the chain,
//! matches on the decoded comment, and deletes by kernel handle — surviving
//! daemon restarts without tracking handles in memory.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};

use async_trait::async_trait;
use ipnetwork::IpNetwork;
use rustables::expr::{
    Bitwise, Cmp, CmpOp, ConnTrackState, Conntrack, ConntrackKey, Immediate, Meta, MetaType,
    Payload, Register, Reject, RejectType, VerdictKind,
};
use rustables::{
    Batch, Chain, ChainPolicy, ChainType, Hook, HookClass, MsgType, Protocol, ProtocolFamily, Rule,
    Table, list_rules_for_chain,
};

use wardnetd_services::inbound_wg::INBOUND_WG_INTERFACE;
use wardnetd_services::routing::firewall::{
    ExceptionAllow, FirewallManager, ZoneIsolationRules, ZoneRules,
};

const TABLE_NAME: &str = "wardnet";
const POSTROUTING: &str = "postrouting";
const PREROUTING: &str = "prerouting";
const FORWARD: &str = "forward";
/// Filter chain on the `input` hook — device→Pi traffic. Carries the Network-Zone
/// admin-UI reject rules (issue #736); DNS/DHCP pass because the chain policy is
/// accept and only :443/:7411 are rejected.
const INPUT: &str = "input";

/// nftables interface-name field width (`IFNAMSIZ`); `meta oifname` loads this
/// many bytes into the comparison register, so masks/values are this length.
pub(crate) const IFNAMSIZ: usize = 16;
/// Wardnet tunnel interface-name prefix. A forwarded packet leaving via a
/// `wg_ward*` interface is taking the tunnel egress path (issue #736).
///
/// The inbound `WireGuard` server interface `wg_wardin0`
/// (`INBOUND_WG_INTERFACE` in `wardnetd_services::inbound_wg`) shares this
/// prefix, but is **explicitly excluded** from the zone-egress drop rule since
/// #810: a zone-denied *remote* device is now a real `Device` row with
/// `ZoneRules`, and its return traffic over `wg_wardin0` is its *inbound*
/// attachment point — not an outbound-provider-tunnel egress path — so it must
/// not be dropped by this gate. The exclusion is expressed by `ANDing` a
/// `oifname != wg_wardin0` match onto the drop rule (see
/// `inbound_wg_iface_exact_value` and the egress gate in `apply_zone_rules`).
const TUNNEL_IFACE_PREFIX: &[u8] = b"wg_ward";
/// Pi admin surfaces a zone may deny (issue #736): the HTTPS admin site (:443)
/// and the plain-HTTP API (:7411). Both are TCP, so denial is a TCP reset.
const ADMIN_UI_PORTS: [u16; 2] = [443, 7411];
/// Comment-UDATA prefix shared by every Network-Zone rule, keyed by device IP.
const ZONE_COMMENT_PREFIX: &str = "wardnet:zone:";

/// Comment on the inbound-`WireGuard` listen-port accept rule (issue #809).
const INBOUND_WG_LISTEN_COMMENT: &str = "wardnet:inbound-wg:listen";

/// Regular (hookless) forward sub-chain that carries the Network-Zone L3
/// isolation rules (issue #737): cross-subnet allows/denies + member isolation.
/// The base `forward` chain jumps to it. The enforcer owns the whole chain and
/// rebuilds it atomically (flush + re-add) on every relevant change.
pub(crate) const ZONE_ISOLATION: &str = "zone_isolation";

/// Regular (hookless) postrouting sub-chain holding the per-exception-pair NAT
/// exemptions. The base `postrouting` chain jumps to it BEFORE any masquerade
/// rule; an `accept` here is terminal for the postrouting hook, so exempted
/// cross-zone exception traffic skips masquerade and reaches the far endpoint
/// with the sender's real source IP.
pub(crate) const ZONE_NATEXEMPT: &str = "zone_natexempt";

/// Comment on the base-forward jump into the `zone_isolation` chain.
const ZONE_ISOJUMP_COMMENT: &str = "wardnet:zone:isojump";

/// Comment on the base-postrouting jump into the `zone_natexempt` chain.
pub(crate) const ZONE_NATEXEMPT_JUMP_COMMENT: &str = "wardnet:zone:natexemptjump";

/// Comment on a `zone_natexempt` per-pair ACCEPT (NAT-exemption) rule.
pub(crate) const ZONE_NATEXEMPT_COMMENT: &str = "wardnet:zone:natexempt";

/// Comment on the stateful cross-zone return-accept rule that sits at the TOP of
/// the `zone_isolation` chain. Cross-zone reply streams (e.g. a Chromecast
/// `sport=8009` return) are `ct state established,related` and would otherwise
/// match no stateless dport allow and be dropped by the cross-subnet deny.
pub(crate) const ZONE_ESTABLISHED_COMMENT: &str = "wardnet:zone:established";

/// Comment on an `wardnet:zone:allow` cross-subnet service ACCEPT rule.
const ZONE_ALLOW_COMMENT: &str = "wardnet:zone:allow";

/// Comment on a `wardnet:zone:xdeny` cross-subnet DROP rule.
const ZONE_XDENY_COMMENT: &str = "wardnet:zone:xdeny";

/// Comment on a `wardnet:zone:memberiso` intra-subnet peer DROP rule.
const ZONE_MEMBERISO_COMMENT: &str = "wardnet:zone:memberiso";

/// Destination-port field offset within the transport header (both TCP and UDP
/// carry `dport` at bytes 2..4), for rendering a port *range* match that the
/// `rustables` `.dport` helper (single-port only) cannot express.
const L4_DPORT_OFFSET: u32 = 2;

/// nftables comment UDATA TLV type (`NFTNL_UDATA_RULE_COMMENT`).
const UDATA_TYPE_COMMENT: u8 = 0;

/// The TCP flags byte sits at offset 13 of the transport header.
const TCP_FLAGS_OFFSET: u32 = 13;
/// `fin | syn | rst` = 0x01 | 0x02 | 0x04.
const TCP_FLAGS_MASK: u8 = 0x07;
/// `IPPROTO_TCP`, compared against `meta l4proto` to scope the flags test.
const IPPROTO_TCP: u8 = 6;
/// `IPPROTO_UDP`, compared against `meta l4proto` to scope a UDP dport range.
const IPPROTO_UDP: u8 = 17;

/// Production [`FirewallManager`] using nftables over netlink.
///
/// Stateless — every method opens its own short-lived netlink socket.
#[derive(Debug, Default)]
pub struct NetlinkFirewallManager;

impl NetlinkFirewallManager {
    /// Create a new netlink-backed firewall manager.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// Build the `inet wardnet` table descriptor (sets family + name; carried into
/// every chain/rule so they address the right object).
pub(crate) fn wardnet_table() -> Table {
    Table::new(ProtocolFamily::Inet).with_name(TABLE_NAME)
}

/// Build a base-chain descriptor under the wardnet table.
fn base_chain(table: &Table, name: &str, hook: HookClass, priority: i32, ty: ChainType) -> Chain {
    Chain::new(table)
        .with_name(name)
        .with_hook(Hook::new(hook, priority))
        .with_type(ty)
        .with_policy(ChainPolicy::Accept)
}

/// A lightweight chain descriptor used to address an existing chain when
/// adding or listing rules (name + family only; no hook needed).
pub(crate) fn chain_ref(table: &Table, name: &str) -> Chain {
    Chain::new(table).with_name(name)
}

/// Encode `comment` as the nftables comment UDATA TLV so it is human-visible in
/// `nft list`: `[type=0][len][value\0]`, where `len` counts the NUL terminator.
#[must_use]
pub fn comment_udata(comment: &str) -> Vec<u8> {
    let bytes = comment.as_bytes();
    // The TLV value is the comment plus a NUL terminator; `len` counts the NUL.
    let value_len = bytes.len() + 1;
    // All callers build comments from a fixed `wardnet:` prefix plus an
    // interface name or a validated IPv4 address, so this never approaches the
    // single-byte TLV length ceiling. Assert in dev/test rather than silently
    // emitting a corrupt TLV; release builds clamp (benign for these callers).
    debug_assert!(
        u8::try_from(value_len).is_ok(),
        "nftables rule comment too long for one UDATA TLV: {value_len} bytes"
    );
    let mut out = Vec::with_capacity(value_len + 2);
    out.push(UDATA_TYPE_COMMENT);
    out.push(u8::try_from(value_len).unwrap_or(u8::MAX));
    out.extend_from_slice(bytes);
    out.push(0); // NUL terminator, counted in len
    out
}

/// Decode the first comment TLV from a rule's userdata, if present. Walks the
/// `[type][len][value]` TLV stream and returns the NUL-stripped UTF-8 comment.
#[must_use]
pub fn parse_comment_udata(data: &[u8]) -> Option<String> {
    let mut i = 0;
    while i + 2 <= data.len() {
        let ty = data[i];
        let len = data[i + 1] as usize;
        let start = i + 2;
        let end = start.checked_add(len)?;
        if end > data.len() {
            break;
        }
        if ty == UDATA_TYPE_COMMENT {
            let mut bytes = &data[start..end];
            if bytes.last() == Some(&0) {
                bytes = &bytes[..bytes.len() - 1];
            }
            return std::str::from_utf8(bytes).ok().map(str::to_owned);
        }
        i = end;
    }
    None
}

/// Comment of a listed rule, if it carries one.
pub(crate) fn rule_comment(rule: &Rule) -> Option<String> {
    rule.get_userdata().and_then(|u| parse_comment_udata(u))
}

/// Run a blocking rustables closure on the Tokio blocking pool.
async fn run_blocking<F, R>(f: F) -> anyhow::Result<R>
where
    F: FnOnce() -> anyhow::Result<R> + Send + 'static,
    R: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| anyhow::anyhow!("nftables netlink task panicked: {e}"))?
}

/// Delete every rule in `chain` whose comment satisfies `pred`. Returns the
/// comments of the deleted rules. Runs in one batch.
fn delete_rules_where(
    chain_name: &str,
    pred: impl Fn(&str) -> bool,
) -> anyhow::Result<Vec<String>> {
    let table = wardnet_table();
    let chain = chain_ref(&table, chain_name);
    let rules = list_rules_for_chain(&chain)
        .map_err(|e| anyhow::anyhow!("list chain {chain_name}: {e}"))?;

    let mut batch = Batch::new();
    let mut removed = Vec::new();
    for rule in rules {
        if let Some(comment) = rule_comment(&rule)
            && pred(&comment)
        {
            batch.add(&rule, MsgType::Del);
            removed.push(comment);
        }
    }
    if !removed.is_empty() {
        batch
            .send()
            .map_err(|e| anyhow::anyhow!("delete rules in {chain_name}: {e}"))?;
    }
    Ok(removed)
}

/// Build the `(mask, value)` pair that matches `meta oifname` against the
/// `wg_ward*` tunnel prefix: the mask keeps only the prefix bytes, and the
/// comparison value is the prefix zero-padded to `IFNAMSIZ`. Combined with a
/// [`Bitwise`] (mask, then compare) this is the netlink equivalent of the CLI's
/// `oifname "wg_ward*"`, so it matches any tunnel index without enumerating live
/// interfaces.
fn tunnel_oifname_mask_and_value() -> (Vec<u8>, Vec<u8>) {
    let mut mask = vec![0u8; IFNAMSIZ];
    let mut value = vec![0u8; IFNAMSIZ];
    for (i, &b) in TUNNEL_IFACE_PREFIX.iter().enumerate() {
        mask[i] = 0xff;
        value[i] = b;
    }
    (mask, value)
}

/// The `meta oifname` comparison value that matches the inbound `WireGuard`
/// server interface (`INBOUND_WG_INTERFACE`) *exactly*: its name zero-padded to
/// `IFNAMSIZ`, the same padding `meta oifname` produces when it loads the full
/// interface name into the comparison register. Paired with a
/// [`Cmp`] using [`CmpOp::Neq`] this excludes `wg_wardin0` from the `wg_ward*`
/// tunnel-egress drop rule (issue #810).
pub(crate) fn inbound_wg_iface_exact_value() -> Vec<u8> {
    let mut value = vec![0u8; IFNAMSIZ];
    for (i, &b) in INBOUND_WG_INTERFACE.as_bytes().iter().enumerate() {
        value[i] = b;
    }
    value
}

/// The device IP a zone rule is keyed on: the final `:`-delimited segment of a
/// `wardnet:zone:<kind>:<ip>` comment, returned only when that segment is a valid
/// IPv4 literal. IPv4 only (no `:` in the address), which matches the daemon's
/// IPv4-only device model.
///
/// The IP check is what keeps the per-device namespace disjoint from the
/// zone-subsystem control rules that share `ZONE_COMMENT_PREFIX` but carry no
/// device IP — the base-forward jump `wardnet:zone:isojump` and the isolation
/// ACCEPT rules `wardnet:zone:allow`. Without it, `isojump` reads as a device
/// "IP" and the orphan sweep deletes the FORWARD→`zone_isolation` jump on every
/// boot, silently disabling all L3 zone isolation.
pub(crate) fn zone_rule_ip(comment: &str) -> Option<&str> {
    if !comment.starts_with(ZONE_COMMENT_PREFIX) {
        return None;
    }
    let tail = comment
        .rsplit(':')
        .next()
        .expect("rsplit always yields at least one element");
    tail.parse::<Ipv4Addr>().is_ok().then_some(tail)
}

/// Parse a `"a.b.c.d/prefix"` string into an [`IpNetwork`] for the network
/// matches of the L3 isolation rules (issue #737).
fn parse_cidr(cidr: &str) -> anyhow::Result<IpNetwork> {
    cidr.parse::<IpNetwork>()
        .map_err(|e| anyhow::anyhow!("invalid CIDR {cidr}: {e}"))
}

/// Append one cross-zone ACCEPT rule to `batch` in the `zone_isolation` chain:
/// `ip saddr <from> ip daddr <to> <proto> dport <start>-<end> accept`. A single
/// port (`start == end`) uses the `.dport` helper; a range is rendered as an
/// explicit `>= start && <= end` match on the transport-header dport field,
/// which the single-port helper cannot express.
fn add_allow_rule(chain: &Chain, allow: &ExceptionAllow, batch: &mut Batch) -> anyhow::Result<()> {
    let proto = match allow.proto.as_str() {
        "tcp" => Protocol::TCP,
        "udp" => Protocol::UDP,
        other => anyhow::bail!("unknown proto {other} in zone allow"),
    };
    let from = parse_cidr(&allow.from_cidr)?;
    let to = parse_cidr(&allow.to_cidr)?;
    let mut rule = Rule::new(chain)?.snetwork(from)?.dnetwork(to)?;

    if allow.port_start == allow.port_end {
        rule = rule.dport(allow.port_start, proto);
    } else {
        // Scope to the transport protocol, then bound the dport field both ways.
        let l4 = match proto {
            Protocol::TCP => IPPROTO_TCP,
            Protocol::UDP => IPPROTO_UDP,
        };
        let dport_payload = || {
            Payload::default()
                .with_base(rustables::sys::NFT_PAYLOAD_TRANSPORT_HEADER)
                .with_offset(L4_DPORT_OFFSET)
                .with_len(2u32)
                .with_dreg(Register::Reg1)
        };
        rule = rule
            .with_expr(Meta::new(MetaType::L4Proto))
            .with_expr(Cmp::new(CmpOp::Eq, [l4]))
            .with_expr(dport_payload())
            .with_expr(Cmp::new(CmpOp::Gte, allow.port_start.to_be_bytes()))
            .with_expr(dport_payload())
            .with_expr(Cmp::new(CmpOp::Lte, allow.port_end.to_be_bytes()));
    }

    rule.accept()
        .with_userdata(comment_udata(ZONE_ALLOW_COMMENT))
        .add_to_batch(batch);
    Ok(())
}

/// Append the stateful cross-zone return-accept rule to `batch`. Renders as
/// `ct state established,related accept`:
///
/// ```text
/// [ ct load state => reg 1 ]
/// [ bitwise reg 1 = ( reg 1 & 0x00000006 ) ^ 0x00000000 ]
/// [ cmp neq reg 1 0x00000000 ]
/// ```
///
/// It MUST be added first (top of the `zone_isolation` chain) so a cross-zone
/// reply stream is accepted before the stateless dport allows and the
/// cross-subnet deny below it. The mask `0x06` is `ESTABLISHED | RELATED`; the
/// register holds the ct state in native (little-endian) byte order, matching
/// the daemon's x86/ARM targets.
fn add_established_rule(chain: &Chain, batch: &mut Batch) -> anyhow::Result<()> {
    let mask = (ConnTrackState::ESTABLISHED | ConnTrackState::RELATED).bits();
    Rule::new(chain)?
        .with_expr(Conntrack::new(ConntrackKey::State))
        .with_expr(Bitwise::new(mask.to_le_bytes(), 0u32.to_le_bytes())?)
        .with_expr(Cmp::new(CmpOp::Neq, 0u32.to_le_bytes()))
        .accept()
        .with_userdata(comment_udata(ZONE_ESTABLISHED_COMMENT))
        .add_to_batch(batch);
    Ok(())
}

/// Populate `batch` with the full desired `zone_isolation` chain in order and
/// return the ordered comment tags of the rules added. The order is:
///
/// 1. the stateful return-accept (`wardnet:zone:established`) — first;
/// 2. the cross-subnet service allows (`wardnet:zone:allow`);
/// 3. the cross-subnet denies (`wardnet:zone:xdeny`);
/// 4. the member-isolation intra-subnet peer denies (`wardnet:zone:memberiso`).
///
/// Pure aside from `batch` mutation (no socket I/O until [`Batch::send`]), so it
/// is unit-testable for both content and ordering.
pub(crate) fn build_zone_isolation_batch(
    iso: &Chain,
    rules: &ZoneIsolationRules,
    batch: &mut Batch,
) -> anyhow::Result<Vec<&'static str>> {
    let mut order: Vec<&'static str> = Vec::new();

    // 1. Stateful return-accept first: cross-zone reply streams
    //    (established,related) bypass the stateless dport allows and the
    //    cross-subnet deny below.
    add_established_rule(iso, batch)?;
    order.push(ZONE_ESTABLISHED_COMMENT);

    // 2. Allows (ACCEPT) so a curated cross-subnet service beats the
    //    cross-subnet deny that follows.
    for allow in &rules.allows {
        add_allow_rule(iso, allow, batch)?;
        order.push(ZONE_ALLOW_COMMENT);
        if allow.bidirectional {
            // The swapped direction on the same ports carries the
            // return/handshake traffic.
            let reverse = ExceptionAllow {
                from_cidr: allow.to_cidr.clone(),
                to_cidr: allow.from_cidr.clone(),
                ..allow.clone()
            };
            add_allow_rule(iso, &reverse, batch)?;
            order.push(ZONE_ALLOW_COMMENT);
        }
    }

    // 3. Cross-subnet denies (DROP).
    for (src, dst) in &rules.deny_pairs {
        let src_net = parse_cidr(src)?;
        let dst_net = parse_cidr(dst)?;
        Rule::new(iso)?
            .snetwork(src_net)?
            .dnetwork(dst_net)?
            .drop()
            .with_userdata(comment_udata(ZONE_XDENY_COMMENT))
            .add_to_batch(batch);
        order.push(ZONE_XDENY_COMMENT);
    }

    // 4. Member-isolation intra-subnet peer denies (DROP). Peer↔peer only:
    //    device→gateway is INPUT, not forward, so the gateway stays reachable.
    for net in &rules.member_isolation_subnets {
        let n = parse_cidr(net)?;
        Rule::new(iso)?
            .snetwork(n)?
            .dnetwork(n)?
            .drop()
            .with_userdata(comment_udata(ZONE_MEMBERISO_COMMENT))
            .add_to_batch(batch);
        order.push(ZONE_MEMBERISO_COMMENT);
    }

    Ok(order)
}

/// The `jump zone_natexempt` rule for the base `postrouting` chain, tagged
/// `wardnet:zone:natexemptjump`. Added by [`init_wardnet_table`] BEFORE any
/// masquerade rule (which [`add_masquerade`] appends later), so an `accept`
/// inside `zone_natexempt` — terminal for the postrouting hook — pre-empts the
/// masquerade and leaves the sender's source IP intact.
pub(crate) fn natexempt_jump_rule(postrouting: &Chain) -> anyhow::Result<Rule> {
    Ok(Rule::new(postrouting)?
        .with_expr(Immediate::new_verdict(VerdictKind::Jump {
            chain: ZONE_NATEXEMPT.to_owned(),
        }))
        .with_userdata(comment_udata(ZONE_NATEXEMPT_JUMP_COMMENT)))
}

/// The `oifname <iface> masquerade` rule for the base `postrouting` chain,
/// tagged `wardnet:<iface>`. Shared by [`add_masquerade`] and the ordering test.
pub(crate) fn masquerade_rule(postrouting: &Chain, iface: &str) -> anyhow::Result<Rule> {
    Ok(Rule::new(postrouting)?
        .oiface(iface)?
        .masquerade()
        .with_userdata(comment_udata(&format!("wardnet:{iface}"))))
}

/// Populate `batch` with the full desired `zone_natexempt` chain: for each
/// `(from, to)` pair, `ip saddr <from> ip daddr <to> accept` AND the reverse
/// `ip saddr <to> ip daddr <from> accept`, both tagged `wardnet:zone:natexempt`.
/// `accept` is terminal for the postrouting hook, so exempted cross-zone
/// exception traffic skips masquerade in both directions. Returns the ordered
/// comment tags (pure aside from `batch` mutation), so it is unit-testable.
pub(crate) fn build_natexempt_batch(
    chain: &Chain,
    pairs: &[(String, String)],
    batch: &mut Batch,
) -> anyhow::Result<Vec<&'static str>> {
    let mut order: Vec<&'static str> = Vec::new();
    for (from, to) in pairs {
        let from_net = parse_cidr(from)?;
        let to_net = parse_cidr(to)?;
        // Forward direction: from -> to.
        Rule::new(chain)?
            .snetwork(from_net)?
            .dnetwork(to_net)?
            .accept()
            .with_userdata(comment_udata(ZONE_NATEXEMPT_COMMENT))
            .add_to_batch(batch);
        order.push(ZONE_NATEXEMPT_COMMENT);
        // Reverse direction: to -> from (the return / receiver-initiated path).
        Rule::new(chain)?
            .snetwork(to_net)?
            .dnetwork(from_net)?
            .accept()
            .with_userdata(comment_udata(ZONE_NATEXEMPT_COMMENT))
            .add_to_batch(batch);
        order.push(ZONE_NATEXEMPT_COMMENT);
    }
    Ok(order)
}

#[async_trait]
impl FirewallManager for NetlinkFirewallManager {
    async fn init_wardnet_table(&self) -> anyhow::Result<()> {
        run_blocking(|| {
            let table = wardnet_table();
            let mut batch = Batch::new();
            // `Add` (NLM_F_CREATE without EXCL) is idempotent: re-adding an
            // existing table/chain is a no-op, matching the CLI's `add` script.
            batch.add(&table, MsgType::Add);
            batch.add(
                &base_chain(
                    &table,
                    POSTROUTING,
                    HookClass::PostRouting,
                    100,
                    ChainType::Nat,
                ),
                MsgType::Add,
            );
            batch.add(
                &base_chain(
                    &table,
                    PREROUTING,
                    HookClass::PreRouting,
                    -100,
                    ChainType::Nat,
                ),
                MsgType::Add,
            );
            batch.add(
                &base_chain(&table, FORWARD, HookClass::Forward, 0, ChainType::Filter),
                MsgType::Add,
            );
            // Input filter chain (device→Pi) for the Network-Zone admin-UI gate
            // (issue #736). Accept policy: only explicit :443/:7411 reject rules
            // bite; DNS/DHCP to the Pi still pass.
            batch.add(
                &base_chain(&table, INPUT, HookClass::In, 0, ChainType::Filter),
                MsgType::Add,
            );
            // Network-Zone L3 isolation (issue #737): a regular (hookless)
            // sub-chain the base forward chain jumps to. `Add` is idempotent, so
            // re-adding the chain is a no-op.
            batch.add(&chain_ref(&table, ZONE_ISOLATION), MsgType::Add);
            // Per-exception-pair NAT exemption: a regular (hookless) sub-chain the
            // base postrouting chain jumps to (before masquerade). Idempotent add.
            batch.add(&chain_ref(&table, ZONE_NATEXEMPT), MsgType::Add);
            batch
                .send()
                .map_err(|e| anyhow::anyhow!("init wardnet table: {e}"))?;

            // Add the base-forward jump into `zone_isolation` only if it is not
            // already present — unlike chains, a duplicate rule would stack, so
            // guard on the comment (mirrors the other base-rule guards).
            let forward = chain_ref(&table, FORWARD);
            let have_jump = list_rules_for_chain(&forward)
                .map_err(|e| anyhow::anyhow!("list forward chain: {e}"))?
                .iter()
                .any(|r| rule_comment(r).as_deref() == Some(ZONE_ISOJUMP_COMMENT));
            if !have_jump {
                let mut jump_batch = Batch::new();
                Rule::new(&forward)?
                    .with_expr(Immediate::new_verdict(VerdictKind::Jump {
                        chain: ZONE_ISOLATION.to_owned(),
                    }))
                    .with_userdata(comment_udata(ZONE_ISOJUMP_COMMENT))
                    .add_to_batch(&mut jump_batch);
                jump_batch
                    .send()
                    .map_err(|e| anyhow::anyhow!("add zone_isolation jump: {e}"))?;
            }

            // Add the base-postrouting jump into `zone_natexempt`, guarded by its
            // comment. CRITICAL ORDERING: this runs during init, before
            // `add_masquerade` appends any masquerade rule, so on the fresh table
            // the jump is postrouting rule #0 and every masquerade lands after it
            // (rustables always appends — there is no insert-at-index). An
            // `accept` reached through this jump is therefore evaluated before
            // masquerade and un-NATs the exempted flow.
            let postrouting = chain_ref(&table, POSTROUTING);
            let have_natexempt_jump = list_rules_for_chain(&postrouting)
                .map_err(|e| anyhow::anyhow!("list postrouting chain: {e}"))?
                .iter()
                .any(|r| rule_comment(r).as_deref() == Some(ZONE_NATEXEMPT_JUMP_COMMENT));
            if !have_natexempt_jump {
                let mut jump_batch = Batch::new();
                natexempt_jump_rule(&postrouting)?.add_to_batch(&mut jump_batch);
                jump_batch
                    .send()
                    .map_err(|e| anyhow::anyhow!("add zone_natexempt jump: {e}"))?;
            }
            Ok(())
        })
        .await?;
        tracing::info!("nftables: wardnet table initialised");
        Ok(())
    }

    async fn flush_wardnet_table(&self) -> anyhow::Result<()> {
        run_blocking(|| {
            let table = wardnet_table();
            // A handle-less `DELRULE` addressed to a chain flushes every rule in
            // that chain (kernel nf_tables semantics) while leaving the chain
            // itself intact. Each chain is flushed in its own batch so an absent
            // or already-empty chain (which the kernel may reject) doesn't abort
            // the flush of the others — flushing is best-effort cleanup.
            for name in [POSTROUTING, PREROUTING, FORWARD, INPUT] {
                let chain = chain_ref(&table, name);
                let mut batch = Batch::new();
                batch.add(&Rule::new(&chain)?, MsgType::Del);
                if let Err(e) = batch.send() {
                    tracing::debug!(chain = name, error = %e, "nftables: flush of chain {name} skipped: {e}");
                }
            }
            Ok(())
        })
        .await?;
        tracing::info!("nftables: wardnet table flushed");
        Ok(())
    }

    async fn add_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        let iface = interface.to_owned();
        run_blocking(move || {
            let table = wardnet_table();
            let chain = chain_ref(&table, POSTROUTING);
            let mut batch = Batch::new();
            // oifname <iface> masquerade comment "wardnet:<iface>"
            // `.oiface()` builds the OifName/Cmp pair and rejects an over-long
            // interface name (IFNAMSIZ); `.masquerade()` appends the verdict. This
            // appends after the `zone_natexempt` jump that `init_wardnet_table`
            // placed at the top of postrouting, so NAT-exempt flows are accepted
            // before this masquerade can fire.
            masquerade_rule(&chain, &iface)?.add_to_batch(&mut batch);
            batch
                .send()
                .map_err(|e| anyhow::anyhow!("add masquerade {iface}: {e}"))?;
            Ok(())
        })
        .await?;
        tracing::info!(interface, "nftables: masquerade rule for {interface} added");
        Ok(())
    }

    async fn remove_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        let iface = interface.to_owned();
        let target = format!("wardnet:{iface}");
        let removed =
            run_blocking(move || delete_rules_where(POSTROUTING, |c| c == target)).await?;
        if removed.is_empty() {
            tracing::warn!(
                interface,
                "nftables: masquerade rule for {interface} not found, nothing to remove"
            );
        } else {
            tracing::info!(
                interface,
                "nftables: masquerade rule for {interface} removed"
            );
        }
        Ok(())
    }

    async fn add_inbound_wg_accept(&self, port: u16) -> anyhow::Result<()> {
        run_blocking(move || {
            // Idempotent add: drop any prior `wardnet:inbound-wg:listen` rule first,
            // then install the one for the current port. Without this, changing
            // `listen_port` while enabled — or re-running `reconcile()` on every
            // daemon restart — would append a fresh accept rule each time and leave
            // stale ones for old ports (only a full disable clears them, since
            // `remove_inbound_wg_accept` deletes ALL matching-comment rules). Mirrors
            // the remove-then-add discipline used for the masquerade rule.
            delete_rules_where(INPUT, |c| c == INBOUND_WG_LISTEN_COMMENT)?;

            let table = wardnet_table();
            let chain = chain_ref(&table, INPUT);
            let mut batch = Batch::new();
            // udp dport <port> accept comment "wardnet:inbound-wg:listen".
            // The `input` chain policy already accepts, so this is a
            // forward-looking safety net (see the trait doc): if the policy is
            // ever tightened, the inbound WireGuard server stays reachable.
            Rule::new(&chain)?
                .dport(port, Protocol::UDP)
                .accept()
                .with_userdata(comment_udata(INBOUND_WG_LISTEN_COMMENT))
                .add_to_batch(&mut batch);
            batch
                .send()
                .map_err(|e| anyhow::anyhow!("add inbound-wg accept on udp/{port}: {e}"))?;
            Ok(())
        })
        .await?;
        tracing::info!(
            port,
            "nftables: inbound-wg UDP accept rule for udp/{port} added"
        );
        Ok(())
    }

    async fn remove_inbound_wg_accept(&self) -> anyhow::Result<()> {
        let removed =
            run_blocking(move || delete_rules_where(INPUT, |c| c == INBOUND_WG_LISTEN_COMMENT))
                .await?;
        if removed.is_empty() {
            tracing::debug!("nftables: no inbound-wg accept rule to remove");
        } else {
            tracing::info!("nftables: inbound-wg accept rule removed");
        }
        Ok(())
    }

    async fn cleanup_legacy_dns_redirects(&self) -> anyhow::Result<()> {
        // Match the legacy `wardnet:dns:*` comments left in prerouting by the
        // pre-#342 DNAT mechanism. This is best-effort startup cleanup: an absent
        // prerouting chain (e.g. a fresh install whose table wasn't initialised
        // yet) is the expected steady state and must not block startup. A genuine
        // netlink failure (no CAP_NET_ADMIN, nf_tables not loaded) is NOT the same
        // thing — surface it at warn so it isn't silently swallowed, but still
        // return Ok so cleanup never gates the daemon.
        let removed = match run_blocking(move || {
            delete_rules_where(PREROUTING, |c| c.starts_with("wardnet:dns:"))
        })
        .await
        {
            Ok(removed) => removed,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "nftables: legacy DNS redirect cleanup skipped (prerouting chain absent, \
                     or netlink unavailable — check CAP_NET_ADMIN / nf_tables): {e}"
                );
                return Ok(());
            }
        };
        for comment in &removed {
            tracing::info!(
                comment,
                "nftables: removed legacy prerouting rule {comment}"
            );
        }
        tracing::debug!(
            removed = removed.len(),
            "nftables: legacy DNS redirect cleanup complete: removed={}",
            removed.len()
        );
        Ok(())
    }

    async fn add_tcp_reset_reject(&self, device_ip: &str) -> anyhow::Result<()> {
        let ip: Ipv4Addr = device_ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid device IP {device_ip}: {e}"))?;
        let comment = format!("wardnet:rst:{device_ip}");
        run_blocking(move || {
            let table = wardnet_table();
            let chain = chain_ref(&table, FORWARD);
            let mut batch = Batch::new();
            // ip saddr <ip> tcp flags & (fin|syn|rst) == 0 reject with tcp reset
            //
            // The `meta l4proto tcp` guard scopes the raw transport-header
            // payload to TCP packets (nft's `tcp flags` keyword does this
            // implicitly; without it we'd match byte 13 of UDP/ICMP too).
            let flags_payload = Payload::default()
                .with_base(rustables::sys::NFT_PAYLOAD_TRANSPORT_HEADER)
                .with_offset(TCP_FLAGS_OFFSET)
                .with_len(1u32)
                .with_dreg(Register::Reg1);
            Rule::new(&chain)?
                .saddr(std::net::IpAddr::V4(ip))
                .with_expr(Meta::new(MetaType::L4Proto))
                .with_expr(Cmp::new(CmpOp::Eq, [IPPROTO_TCP]))
                .with_expr(flags_payload)
                .with_expr(Bitwise::new([TCP_FLAGS_MASK], [0u8])?)
                .with_expr(Cmp::new(CmpOp::Eq, [0u8]))
                .with_expr(Reject::default().with_type(RejectType::TcpRst))
                .with_userdata(comment_udata(&comment))
                .add_to_batch(&mut batch);
            batch
                .send()
                .map_err(|e| anyhow::anyhow!("add tcp reset reject {ip}: {e}"))?;
            Ok(())
        })
        .await?;
        tracing::debug!(
            device_ip,
            "nftables: TCP RST reject rule for {device_ip} added"
        );
        Ok(())
    }

    async fn remove_tcp_reset_reject(&self, device_ip: &str) -> anyhow::Result<()> {
        let target = format!("wardnet:rst:{device_ip}");
        let dip = device_ip.to_owned();
        let removed = run_blocking(move || delete_rules_where(FORWARD, |c| c == target)).await?;
        if removed.is_empty() {
            tracing::debug!(
                device_ip = dip,
                "nftables: TCP RST reject rule for {dip} not found, nothing to remove"
            );
        } else {
            tracing::debug!(
                device_ip = dip,
                "nftables: TCP RST reject rule for {dip} removed"
            );
        }
        Ok(())
    }

    async fn apply_zone_rules(
        &self,
        device_ip: &str,
        rules: ZoneRules,
        lan_interface: &str,
    ) -> anyhow::Result<()> {
        let ip: Ipv4Addr = device_ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid device IP {device_ip}: {e}"))?;
        let egress_comment = format!("wardnet:zone:egress:{device_ip}");
        let adminui_comment = format!("wardnet:zone:adminui:{device_ip}");
        let suffix = format!(":{device_ip}");
        let lan = lan_interface.to_owned();
        run_blocking(move || {
            let table = wardnet_table();
            let forward = chain_ref(&table, FORWARD);
            let input = chain_ref(&table, INPUT);
            let mut batch = Batch::new();
            let mut ops = 0usize;

            // 1. Delete this IP's existing zone rules in both chains so the apply
            //    is a clean replace. Both delete and add ride the same batch, so
            //    the swap lands in one atomic netlink transaction.
            for chain_name in [FORWARD, INPUT] {
                let chain = chain_ref(&table, chain_name);
                let existing = list_rules_for_chain(&chain)
                    .map_err(|e| anyhow::anyhow!("list chain {chain_name}: {e}"))?;
                for rule in existing {
                    if let Some(comment) = rule_comment(&rule)
                        && comment.starts_with(ZONE_COMMENT_PREFIX)
                        && comment.ends_with(&suffix)
                    {
                        batch.add(&rule, MsgType::Del);
                        ops += 1;
                    }
                }
            }

            // 2. Egress gate: drop forwarded packets leaving via a forbidden
            //    path. `allowed_targets` is never empty, so at most one applies.
            if !rules.allow_tunnel {
                // ip saddr <ip> oifname "wg_ward*" oifname != "wg_wardin0" drop
                //
                // The prefix match (masked Bitwise + Eq) catches every outbound
                // provider tunnel `wg_ward*`. The second, unmasked oifname load
                // + Neq excludes the inbound server interface `wg_wardin0`: a
                // zone-denied remote device's return traffic over its own
                // inbound attachment point is not tunnel egress and must not be
                // dropped here (issue #810). nftables evaluates the two loads in
                // order, each into the comparison register, so the rule matches
                // only when BOTH the prefix Eq and the exact Neq hold.
                let (mask, value) = tunnel_oifname_mask_and_value();
                Rule::new(&forward)?
                    .saddr(IpAddr::V4(ip))
                    .with_expr(Meta::new(MetaType::OifName))
                    .with_expr(Bitwise::new(mask, vec![0u8; IFNAMSIZ])?)
                    .with_expr(Cmp::new(CmpOp::Eq, value))
                    .with_expr(Meta::new(MetaType::OifName))
                    .with_expr(Cmp::new(CmpOp::Neq, inbound_wg_iface_exact_value()))
                    .drop()
                    .with_userdata(comment_udata(&egress_comment))
                    .add_to_batch(&mut batch);
                ops += 1;
            }
            if !rules.allow_direct {
                // ip saddr <ip> oifname <lan_interface> drop
                Rule::new(&forward)?
                    .saddr(IpAddr::V4(ip))
                    .oiface(&lan)?
                    .drop()
                    .with_userdata(comment_udata(&egress_comment))
                    .add_to_batch(&mut batch);
                ops += 1;
            }

            // 3. Admin-UI gate: refuse device→Pi :443/:7411 with a TCP reset so
            //    the client sees "connection refused", not a silent hang. DNS
            //    (:53) and DHCP are untouched — the accept-policy input chain
            //    passes them.
            if !rules.admin_ui_reachable {
                for port in ADMIN_UI_PORTS {
                    Rule::new(&input)?
                        .saddr(IpAddr::V4(ip))
                        .dport(port, Protocol::TCP)
                        .with_expr(Reject::default().with_type(RejectType::TcpRst))
                        .with_userdata(comment_udata(&adminui_comment))
                        .add_to_batch(&mut batch);
                    ops += 1;
                }
            }

            // An empty batch (no prior rules, nothing to add) would be a wasted
            // — and possibly rejected — netlink round-trip; skip it.
            if ops > 0 {
                batch
                    .send()
                    .map_err(|e| anyhow::anyhow!("apply zone rules for {ip}: {e}"))?;
            }
            Ok(())
        })
        .await?;
        tracing::debug!(
            device_ip,
            allow_direct = rules.allow_direct,
            allow_tunnel = rules.allow_tunnel,
            admin_ui_reachable = rules.admin_ui_reachable,
            "nftables: applied zone rules for {device_ip}"
        );
        Ok(())
    }

    async fn remove_zone_rules(&self, device_ip: &str) -> anyhow::Result<()> {
        let key = device_ip.to_owned();
        let removed = run_blocking(move || {
            let mut all = Vec::new();
            for chain_name in [FORWARD, INPUT] {
                // Match on the parsed device-IP key rather than a raw `:<ip>`
                // suffix: `zone_rule_ip` rejects any comment whose tail isn't a
                // valid IPv4, so a control rule like `wardnet:zone:isojump` can
                // never be selected here even if `key` were "isojump".
                let r = delete_rules_where(chain_name, |c| zone_rule_ip(c) == Some(key.as_str()))?;
                all.extend(r);
            }
            Ok(all)
        })
        .await?;
        if removed.is_empty() {
            tracing::debug!(
                device_ip,
                "nftables: no zone rules for {device_ip} to remove"
            );
        } else {
            tracing::debug!(
                device_ip,
                count = removed.len(),
                "nftables: removed {} zone rule(s) for {device_ip}",
                removed.len()
            );
        }
        Ok(())
    }

    async fn list_zone_rule_ips(&self) -> anyhow::Result<Vec<String>> {
        run_blocking(|| {
            let table = wardnet_table();
            let mut ips: HashSet<String> = HashSet::new();
            for chain_name in [FORWARD, INPUT] {
                let chain = chain_ref(&table, chain_name);
                let rules = match list_rules_for_chain(&chain) {
                    Ok(rules) => rules,
                    Err(e) => {
                        // An absent chain (table not yet initialised) is the
                        // expected steady state on a fresh install — skip it
                        // rather than fail the reconcile.
                        tracing::debug!(
                            chain = chain_name,
                            error = %e,
                            "nftables: listing zone rules in {chain_name} skipped: {e}"
                        );
                        continue;
                    }
                };
                for rule in rules {
                    if let Some(comment) = rule_comment(&rule)
                        && let Some(ip) = zone_rule_ip(&comment)
                    {
                        ips.insert(ip.to_owned());
                    }
                }
            }
            Ok(ips.into_iter().collect())
        })
        .await
    }

    async fn apply_zone_isolation(&self, rules: ZoneIsolationRules) -> anyhow::Result<()> {
        let allows = rules.allows.len();
        let denies = rules.deny_pairs.len();
        let members = rules.member_isolation_subnets.len();
        let exempts = rules.nat_exempt_pairs.len();
        run_blocking(move || {
            let table = wardnet_table();
            let iso = chain_ref(&table, ZONE_ISOLATION);
            let natexempt = chain_ref(&table, ZONE_NATEXEMPT);
            let mut batch = Batch::new();

            // 1. Flush both owned sub-chains: the enforcer owns them and rebuilds
            //    the full desired state, so drop everything currently in them. A
            //    handle-less DELRULE addressed to a chain flushes it while
            //    leaving the chain intact. Each flush rides its own batch so an
            //    absent/empty chain doesn't abort the rebuild.
            for chain in [&iso, &natexempt] {
                let mut flush = Batch::new();
                flush.add(&Rule::new(chain)?, MsgType::Del);
                if let Err(e) = flush.send() {
                    tracing::debug!(error = %e, "nftables: flush of owned sub-chain skipped: {e}");
                }
            }

            // 2. Rebuild the zone_isolation chain: stateful return-accept first,
            //    then allows, cross-subnet denies, and member-isolation denies.
            build_zone_isolation_batch(&iso, &rules, &mut batch)?;

            // 3. Rebuild the zone_natexempt chain: both directions per pair.
            build_natexempt_batch(&natexempt, &rules.nat_exempt_pairs, &mut batch)?;

            batch
                .send()
                .map_err(|e| anyhow::anyhow!("rebuild zone sub-chains: {e}"))?;
            Ok(())
        })
        .await?;
        tracing::debug!(
            allows,
            denies,
            members,
            exempts,
            "nftables: rebuilt zone_isolation + zone_natexempt chains"
        );
        Ok(())
    }

    async fn check_tools_available(&self) -> anyhow::Result<()> {
        run_blocking(|| {
            rustables::list_tables()
                .map_err(|e| anyhow::anyhow!("nftables netlink socket not working: {e}"))?;
            Ok(())
        })
        .await?;
        tracing::info!("nftables: netlink socket available");
        Ok(())
    }

    async fn destroy_wardnet_table(&self) -> anyhow::Result<()> {
        let result = run_blocking(|| {
            let table = wardnet_table();
            let mut batch = Batch::new();
            batch.add(&table, MsgType::Del);
            batch
                .send()
                .map_err(|e| anyhow::anyhow!("destroy wardnet table: {e}"))?;
            Ok(())
        })
        .await;
        match result {
            Ok(()) => tracing::info!("nftables: wardnet table destroyed"),
            // Ignore errors when the table doesn't exist (first run / already gone).
            Err(e) => {
                tracing::debug!(error = %e, "nftables: ignoring error during table destruction: {e}");
            }
        }
        Ok(())
    }
}
