---
status: accepted
date: 2026-07-02
issue: "#737 (Phase 2 · CI-3 of epic #244 — Network Zones)"
---

# ADR: Network Zone deep isolation — per-zone subnets, a whole-chain L3 enforcer, a cross-zone exception engine, and why there is no mDNS reflector

---

## Context

`#735` built the Network Zone data model and an API-level routing gate, recording
— but not enforcing — `isolation_stance`, `member_isolation`, and
`subnet: Option<ZoneSubnet>`. `#736` added the packet layer that works on a
**flat shared subnet**: per-device nftables egress + admin-UI gating, keyed by
device IP, live-reloaded by a dedicated `ZoneEnforcementService` +
`ZoneEnforcementListener`.

`#737` is the "real guest network": the isolation that only works when **Wardnet
owns DHCP**, so it can put each zone on its **own subnet** and route (and filter)
the traffic between them. It gives the recorded-only fields teeth — per-zone
subnets, cross-zone default-deny, opt-in isolate-members (per-device `/32` +
per-member proxy-neighbour entries), a cross-zone exception engine, and a casting preset.

Several decisions here are hard to reverse (an addressing convention, a new
firewall chain, a new event) or surprising (there is deliberately **no** mDNS
reflector despite the issue text asking for one), so they are recorded here.

## Decision

### 1. Subnets are admin-assigned; `None` means the base LAN subnet

A zone's `subnet` is opt-in. `subnet = None` keeps the zone on the **base LAN
subnet** (the Pi's own subnet, shared with every other `None` zone) — exactly the
`#736` shared-subnet behavior. `subnet = Some(cidr)` gives the zone its own
scope: the Pi aliases a gateway (`.1` of the cidr) on the LAN interface, DHCP
hands leases from the cidr, and cross-subnet traffic is default-denied.

All three seed zones ship `None`, so **upgrading changes no packet's fate** until
an admin gives a zone a subnet. Deep isolation is a deliberate, per-zone opt-in
rather than a migration that silently re-addresses the network. The API validates
the cidr for overlap (against the base subnet and other zones) and suggests a free
`/24` from `10.44.0.0/16`, but stores the explicit value.

The Pi is usually **not** the edge router — it sits behind the home router, whose
subnet is the base subnet. Zone subnets are carved private ranges the Pi is the
gateway and NAT for; the anchor (Trusted) is expected to stay on the base subnet
while Guest/IoT opt into carved subnets.

### 2. DHCP-mode gates everything; storing a subnet while DHCP is off is inert, not rejected

Per-zone subnets, gateway aliasing, cross-subnet deny, and proxy-neighbour
entries all require
Wardnet to control addressing, i.e. `dhcp_enabled`. When Wardnet is not the DHCP
server the enforcer applies **none** of it and degrades to `#736` (egress +
admin-UI gating, which work on any topology). Storing a subnet while DHCP is off
is **allowed** (recorded intent) and surfaced as inactive; it activates
automatically when DHCP is enabled. We chose allow-and-no-op over a hard 409 so
operators can configure zones before flipping DHCP on, without a forced ordering.

### 3. A whole-chain L3 enforcer, not per-rule bookkeeping

The L3 isolation lives in a dedicated nftables **regular chain `zone_isolation`**
that the base `forward` chain jumps to. On any relevant change the enforcer
recomputes the **entire** desired state and the firewall **flushes and rebuilds
that chain** in a fixed order: **exception ACCEPTs first, then cross-subnet DROPs,
then member-isolation DROPs.** Because the enforcer owns the whole chain, ordering
(allows before denies) is trivially correct and there is no per-rule identity
bookkeeping — unlike the per-device `wardnet:zone:*:<ip>` rules of `#736`, which
stay incremental in the `forward`/`input` base chains.

`deny_pairs` are every ordered pair of distinct subnets (base ∪ each zone
subnet), so guest↔trusted and guest↔base are both dropped. `member_isolation`
adds an intra-subnet `saddr net daddr net drop` — safe because device→gateway
traffic is `input`, not `forward`, so the gateway stays reachable; only peer↔peer
forwarding is dropped.

### 4. The cross-zone exception engine and the casting preset

A `zone_exceptions` row allows one endpoint to reach another across the deny. Each
of `from`/`to` is `(kind: device | zone, id)`; the service is a **named preset**
(`casting`) or a custom port list; rules are **stateful** (conntrack auto-allows
the return path) with an explicit **`bidirectional`** flag for protocols where the
far side also initiates. The enforcer resolves endpoints to cidrs (device → `/32`,
zone → its subnet or the base subnet) and expands the service to ACCEPT rules
emitted ahead of the deny. The **casting** preset expands to mDNS 5353/udp,
SSDP/DLNA 1900/udp, Chromecast 8008-8009/9000/tcp, and AirPlay 7000/7100/tcp,
bidirectional.

### 5. No mDNS reflector — casting works via exceptions on a shared L2 segment

The issue asks for an Avahi-style mDNS reflector. **We deliberately do not build
one.** All zones share **one physical LAN interface via IP aliasing** (VLAN is an
epic non-goal), so they share a single L2 broadcast domain: mDNS/SSDP multicast is
flooded to every device regardless of IP subnet — even under isolate-members,
where the per-device `/32` + proxy-neighbour entries affect only *unicast* ARP,
not multicast.
Discovery therefore already crosses the subnet split for free. What blocks casting
by default is the **L3 unicast deny** on the routed connection; what enables it is
the **exception allow-rule**. A reflector would add a large, CI-only multicast
relay that is a no-op on this topology.

A reflector only becomes load-bearing when *discovery multicast itself* is
blocked — AP "client isolation" on a guest SSID, or a future move to real VLAN
segmentation. Both are out of scope here and left as a follow-up.

## Non-goals / documented limits

- **No mDNS reflector** (see §5) — follow-up for AP-client-isolation / VLAN.
- **No VLAN / 802.1Q** — epic non-goal; the shared-L2 reality is why §5 holds.
- **Isolate-members is cooperating-devices-only.** A device that ignores its
  `/32` and self-assigns a wider mask can ARP a same-subnet peer directly,
  bypassing the Pi. Breaking that would require ARP spoofing, an epic non-goal.
- **Degraded when Wardnet is not the DHCP server** (see §2): only `#736`
  shared-subnet gating applies.
- **Zone-move re-IP is best-effort.** On a zone change the enforcer releases the
  device's lease and flushes conntrack so it re-DISCOVERs into the new subnet, but
  the Pi cannot force an instant client renew — there is a brief blip and, for
  clients that ignore the release, a wait until lease renewal.

## Consequences

- **Blast radius:** `FirewallManager` gains `apply_zone_isolation` + the
  `zone_isolation` chain; `PolicyRouter` gains interface aliasing, per-member
  proxy-neighbour (pneigh) entries, and `/32` host routes; the enforcer gains the exception repository, a DHCP-service
  handle (lease revoke on zone move), and the LAN IP; a new
  `WardnetEvent::ZoneExceptionsChanged` is added to the bus and both exhaustive
  matches.
- **Verification:** the decision logic (which subnets, deny pairs, exception
  expansion, alias/pneigh/host-route choices) is unit-tested natively with
  recording firewall + policy-router spies over real in-memory repositories. The
  rustables rendering and the netlink address/route ops compile only on Linux, so
  they are gated behind `make check-daemon` / CI.
- **Reversibility:** blanking a zone's subnet removes its gateway alias and
  collapses it back onto the base subnet; disabling DHCP degrades cleanly to
  `#736`. The addressing convention (`10.44.0.0/16` suggestions, `.1` gateway) is
  a default, not a stored contract.

## Amendment (issue #1107): per-member proxy-neighbour entries, not `proxy_arp=1`

The original implementation realised the ARP-interception leg of isolate-members
with the interface-wide `net.ipv4.conf.<lan>.proxy_arp=1` sysctl. That mechanism
was doubly wrong:

- **It answered for everything a tunnel-bound device probed.** The kernel's
  proxy-ARP decision is a FIB lookup of the ARP *target* using the ARP *sender*
  as source. For a tunnel-bound sender that lookup hits its `from <ip> lookup
  <table>` policy rule, whose only route egresses `wg_ward*` — a different
  interface than the LAN — so the Pi proxy-replied for **any** target,
  including the sender's own gratuitous ARP (macOS "duplicate IP", DHCP
  decline loops) and same-LAN peers (traffic hijacked into a table with no LAN
  routes).
- **It never fired for the intended case.** A same-zone, non-tunnel-bound peer's
  lookup resolves out the same interface the ARP arrived on, and plain
  `proxy_arp` never answers same-interface lookups.

The enforcer now installs **per-member proxy-neighbour entries**
(`ip neigh add proxy <member-ip> dev <lan>`) instead: the Pi answers ARP for
exactly the members of isolate-members zones, regardless of route egress
interface. The desired set is reconciled (with pruning of stale entries) in
`reconcile_isolation`, and startup reconcile clears any `proxy_arp=1` left
behind by a pre-#1107 daemon. The interface-wide sysctl must never be
re-enabled.
