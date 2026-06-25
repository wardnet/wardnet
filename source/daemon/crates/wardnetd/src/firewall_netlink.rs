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

use std::net::Ipv4Addr;

use async_trait::async_trait;
use rustables::expr::{Bitwise, Cmp, CmpOp, Meta, MetaType, Payload, Register, Reject, RejectType};
use rustables::{
    Batch, Chain, ChainPolicy, ChainType, Hook, HookClass, MsgType, ProtocolFamily, Rule, Table,
    list_rules_for_chain,
};

use wardnetd_services::routing::firewall::FirewallManager;

const TABLE_NAME: &str = "wardnet";
const POSTROUTING: &str = "postrouting";
const PREROUTING: &str = "prerouting";
const FORWARD: &str = "forward";

/// nftables comment UDATA TLV type (`NFTNL_UDATA_RULE_COMMENT`).
const UDATA_TYPE_COMMENT: u8 = 0;

/// The TCP flags byte sits at offset 13 of the transport header.
const TCP_FLAGS_OFFSET: u32 = 13;
/// `fin | syn | rst` = 0x01 | 0x02 | 0x04.
const TCP_FLAGS_MASK: u8 = 0x07;
/// `IPPROTO_TCP`, compared against `meta l4proto` to scope the flags test.
const IPPROTO_TCP: u8 = 6;

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
fn wardnet_table() -> Table {
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
fn chain_ref(table: &Table, name: &str) -> Chain {
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
fn rule_comment(rule: &Rule) -> Option<String> {
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
            batch
                .send()
                .map_err(|e| anyhow::anyhow!("init wardnet table: {e}"))?;
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
            for name in [POSTROUTING, PREROUTING, FORWARD] {
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
            // interface name (IFNAMSIZ); `.masquerade()` appends the verdict.
            Rule::new(&chain)?
                .oiface(&iface)?
                .masquerade()
                .with_userdata(comment_udata(&format!("wardnet:{iface}")))
                .add_to_batch(&mut batch);
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
