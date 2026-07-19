---
status: accepted
date: 2026-07-19
issue: "feature/pass-switchback (follows #737 cross-zone exceptions, #961 zone_isolation-jump fix)"
---

# ADR: Switchback — making a cross-zone exception actually reach a tunnel-bound device, plus the stateful return the stateless allow-rules were missing

---

## Context

`#737` built the cross-zone **exception engine** and the **casting preset**: an
admin grants `Family ↔ Entertainment` a set of ports, and the
`ZoneEnforcementService` renders `wardnet:zone:allow` ACCEPT rules into the
`zone_isolation` chain so the routed unicast survives the cross-subnet
`xdeny` drop. On paper a phone in one zone can then cast to a TV in another.

In practice it did not work, for two independent reasons that only surfaced
under live packet capture (a phone in **Family** `192.168.200.0/24` casting to a
Xiaomi box in **Entertainment** `192.168.201.0/24`, both bound to the same
WireGuard tunnel):

1. **The routing layer swallows the packet before the firewall ever sees it.**
   A tunnel-bound device has a source `ip rule from <device_ip> lookup
   <tunnelTable>`, and that table holds only `default dev wg_wardX`. The phone's
   `→ box:8009` SYN matches the default and is sent *up the tunnel* — it never
   reaches the forward chain where the `zone:allow` rule lives. The exception's
   ports are irrelevant because the traffic is gone before filtering.

2. **The allow-rules are stateless, so the reply stream is dropped.** The
   `zone:allow` rules match on `dport ∈ {cast ports}`. The receiver's replies
   carry `sport = 8009, dport = <ephemeral>` and match no allow-rule; once
   isolation is actually enforced they fall to the `xdeny` cross-subnet drop.
   (This stayed invisible while `#961`'s bug left the `zone_isolation` jump
   reconciled away — with isolation off, everything passed.)

`#961` (on this branch) fixes the jump so isolation runs. That is a prerequisite,
not a fix for either problem above.

## Decision

Ship two coordinated changes alongside `#961`, under the name **Switchback**.

### 1. Switchback: a scoped, per-device routing carve-out

For a tunnel-bound device, install a **higher-precedence** source+destination
`ip rule` for each peer its zone has an exception with:

```
from <device_ip> to <peer_cidr> lookup main   priority 1000
```

`main` (table 254) has the connected LAN routes, so the cross-zone unicast is
delivered locally and reaches the forward chain. Priority `1000` sits below the
kernel-auto-assigned tunnel source rules (~32764/32765) so it wins, and above
`local` (0) so the Pi's own addresses are unaffected.

- **Scoped to exceptions, not blanket.** The carve-out covers only the peer
  CIDRs the device's zone actually has an exception with — routing encodes the
  same zone-pair policy the firewall does (defense-in-depth). A tunnel-bound
  device with no exceptions keeps *all* its traffic on the tunnel.
- **Subnet-granular; the firewall still gates ports.** An `ip rule` cannot match
  L4 ports, so the carve-out routes the whole peer subnet locally and the
  `zone_isolation` chain decides which ports pass. Routing = reachability,
  firewall = policy.
- **Owned by the routing service, driven by the zone enforcer.** The
  `ZoneEnforcementService` already reacts to exception / zone-subnet /
  device-zone changes and resolves an endpoint to a CIDR (`resolve_endpoint_cidr`).
  It computes each device's target CIDR set from the same exceptions (expanding
  a zone endpoint to its member devices, honouring `bidirectional`) and pushes it
  to `RoutingService::set_switchback_targets` — mirroring the existing
  `ZoneEnforcement → routing.apply_rule_for_device` callback. The routing service
  materialises the `ip rule`s **only while the device is tunnel-bound** and tears
  them down on unbind, IP change, or removal. The firewall allow-rules and the
  routing carve-out are therefore computed from one source and cannot drift.

### 2. A stateful cross-zone return accept

Prepend `ct state established,related accept` to the **top of the
`zone_isolation` chain**. This carries the reply stream of any accepted cross-zone
flow. It is scoped to that chain (cross-zone traffic only), so the base-`forward`
tunnel-egress gate and the `input` admin-UI gate are untouched.

## Considered options

- **Blanket LAN carve-out** (route *all* local subnets locally for every
  tunnel-bound device, letting the firewall be the sole gate). Simpler and
  auto-tracking, but a shared tunnel table is used by devices from *different*
  zones, so table-level routes would give a device carve-outs its own zone was
  never granted — collapsing to firewall-only enforcement. Rejected in favour of
  per-device rules that preserve the routing-level policy.
- **LAN routes inside the per-tunnel table** instead of per-device `ip rule`s.
  Same shared-table problem: it cannot express per-device (per-zone) scope.
- **Stateless reverse-allow rules** (emit `sport`-matched allows for the return
  direction) instead of a conntrack accept. More rules, fragile, and still wrong
  for related (non-reply) traffic. Rejected for the standard stateful pattern.

## Consequences

- Switchback, the stateful return, and `#961` must ship together: none makes
  cross-zone casting work alone.
- Inter-zone traffic still hits the base `oifname <lan> masquerade`, so the
  receiver sees the gateway IP, not the real sender. Basic app-casting works;
  sender-hosted flows (screen mirror, local-file casting) may not. Un-NATing
  inter-zone LAN traffic is a separate, deliberately deferred concern.
- The routing service now holds per-device switchback target state and prunes
  orphaned priority-1000 rules on reconcile.
