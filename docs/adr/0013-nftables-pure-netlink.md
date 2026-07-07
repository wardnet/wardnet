---
status: accepted
date: 2026-06-23
issue: "#307 — records the design shipped on `feature/nftables-netlink-307`"
---

# ADR: nftables management via pure netlink (rustables)

---

## Context

The daemon's policy routing and conntrack were migrated from CLI tools to direct
netlink sockets (`rtnetlink`) as part of #77, eliminating CLI parsing fragility.
**nftables was the last remaining CLI dependency** — `NftablesFirewallManager`
shelled out to `nft` for masquerade NAT (postrouting), TCP-RST reject (forward),
table/chain lifecycle, and legacy DNS-redirect cleanup, parsing `nft -a list`
text to recover rule handles for deletion.

Finishing the "no CLI tools" direction means talking nftables netlink directly.
Three Rust options exist:

- **`rustables`** — pure-Rust nftables over netlink; a fork of nftnl-rs that
  builds the netlink messages itself. No C library at runtime.
- **`nftnl`** — FFI bindings to the C `libnftnl`; mature, but reintroduces a C
  build + runtime dependency.
- **`nftables`** (libnftables JSON) — wraps the same engine the `nft` CLI uses;
  effectively keeps the CLI's C dependency, just behind a JSON API.

## Decision

**Replace the `nft` shell-out with a `NetlinkFirewallManager` backed by
`rustables`, behind the unchanged `FirewallManager` trait.**

- **`rustables` over `nftnl`/libnftables.** It matches the "pure netlink, no C"
  philosophy established by the `rtnetlink` migration: nothing C links at
  *runtime*. The trade-off is rustables' self-described "rough edges" and a
  **build-time** dependency (below). We accepted this after a real-kernel spike
  proved the full ruleset is byte-equivalent to the CLI's.
- **Trait boundary unchanged.** `FirewallManager` (9 methods) is the stable
  seam; `RoutingServiceImpl`, the test mocks, and the no-op backend are
  untouched. Swapping back to the CLI impl would be a one-line change in
  `main.rs` — which is why this is recorded but not heavily defended.
- **Rule identity via the nftables comment UDATA TLV.** Each wardnet rule is
  tagged with a comment encoded in the standard nftables comment userdata TLV
  (`type=0`, length-incl-NUL, value). Removal lists the chain over netlink and
  matches the decoded comment, deleting by kernel handle — preserving the
  restart-survivable idempotency the old text-parsing gave, *and* keeping the
  comments human-visible in `nft list` for operator debugging.

## Consequences

- **Build-time C toolchain (new).** `rustables`' `build.rs` runs `bindgen`
  against `<linux/netfilter/nf_tables.h>`, so every build path now needs
  `clang`/`libclang` (kernel uapi headers come from `libc6-dev`). This touches
  the `make *-container` targets (`rust:1.96` image), the `setup-rust` CI action,
  and — most fiddly — the **aarch64 cross-compile**, which needs
  `BINDGEN_EXTRA_CLANG_ARGS` pointed at the cross sysroot. The `rtnetlink` path
  had no bindgen; this is the main cost of the decision.
- **Supply chain.** `rustables` is a lower-population crate with a `build.rs`.
  It is covered by the existing `cargo audit` gate (`.github/workflows/security.yml`)
  and the cargo Dependabot watch, so future advisories surface automatically.
- **TCP-flags match is a raw payload.** rustables exposes no high-level TCP-flags
  field, so `tcp flags & (fin|syn|rst) == 0` is built as a raw transport-header
  payload (offset 13, len 1) + bitwise + cmp. It **must** be preceded by a
  `meta l4proto tcp` guard — otherwise it would also match byte-13 of UDP/ICMP
  packets. With the guard, `nft` delinearizes it back to `tcp flags ! fin,syn,rst`,
  identical to the CLI rule.
- **Runtime no longer needs the `nft` binary** — only the in-kernel `nf_tables`
  module. The `nftables` apt package is retained in the images as an operator
  debugging aid (rules are intentionally `nft list`-visible), not a daemon
  dependency.
- **No host-independent unit tests for the socket calls.** Like
  `NetlinkPolicyRouter`, netlink has no mockable boundary; only the pure helpers
  (comment-TLV codec) are unit-tested. Real-kernel behaviour is covered by the
  e2e docker harness.
