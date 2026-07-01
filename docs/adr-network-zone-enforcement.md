# ADR: Network Zone packet enforcement — a decoupled nftables enforcer, and closing the default-policy caveat via an event + callback

**Status**: Accepted
**Date**: 2026-07-01
**Issue**: #736 (Phase 1 · CI-2 of epic #244 — Network Zones)

---

## Context

#735 built the Network Zone data model, service, and API-level gate but changed
no packet's fate (see `adr-network-zone-isolation.md`). #736 is the **packet
layer**: it makes a zone *mean something* on a single flat LAN — the enforcement
that works even when Wardnet is not the DHCP server — via per-zone **egress
gating** and **admin-UI gating**, live-reloaded on zone changes with no daemon
restart.

Several decisions here are hard to reverse (an event added to the bus, a new
firewall trait surface) or surprising (the enforcer deliberately duplicates a
little zone logic that already lives in `DeviceService`, and it reaches back into
`RoutingService` rather than the reverse), so they are recorded here.

## Decision

### 1. A dedicated enforcer, decoupled from the routing engine

Zone enforcement is a **separate** event-bus subscriber
(`ZoneEnforcementService` + `ZoneEnforcementListener`), not an extension of the
routing listener. The routing listener's own code already said the CI-2/CI-3
enforcers "subscribe separately," and the two enforce different layers (packet
gating vs kernel policy routing) that must not block each other. The enforcer
shares the `FirewallManager` and `PolicyRouter` backends with the routing
service (they cooperate on the one `wardnet` nftables table and per-device
conntrack) but owns its own rules.

The egress gate depends **only** on the zone's `allowed_targets` — a direct-only
zone drops tunnel egress regardless of the device's current routing target — so
the enforcer needs no knowledge of the resolved routing target for its packet
rules. That keeps the packet layer a pure function of `(device_ip, zone)`.

### 2. Chain layout and verdicts

- **Egress gate** — forward-chain **drop** keyed by device IP
  (`wardnet:zone:egress:<ip>`). A tunnel-forbidding zone drops packets leaving
  via `wg_ward*` (an `oifname` prefix match built from `meta oifname` + a
  bitwise mask + compare, the netlink equivalent of `oifname "wg_ward*"`, so it
  matches any tunnel index without enumerating live interfaces). A
  direct-forbidding zone drops packets leaving via the WAN interface. The issue
  says "drop," and a silent drop is the right default for egress.
- **Admin-UI gate** — a new `input` base chain (accept policy) carries
  **reject-with-tcp-reset** rules (`wardnet:zone:adminui:<ip>`) for device→Pi
  :443 and :7411 when `admin_ui_reachable = false`. The AC says
  "connection-refused," which is a TCP reset, not a silent drop. DNS (:53) and
  DHCP pass untouched because only those two ports are rejected.

Rule identity is nftables comment UDATA keyed by device IP — the same
restart-survivable scheme the masquerade/RST rules already use — so rules are
rebuilt from the database on startup and never tracked in memory.

### 3. Closing the default-policy caveat: an event + enforcer callback

`adr-network-zone-isolation.md` names exactly one edge the #735 write-time gate
cannot catch: a change to the **global default routing policy** re-resolves every
stored `Default` rule at once, which can bind a device to a target its zone
forbids. It assigned reconciling that to the CI-2 enforcer.

We close it with a new `WardnetEvent::DefaultPolicyChanged`, emitted by
`RoutingService::set_default_policy`, to which the enforcer subscribes: for each
`Default`-ruled device whose zone forbids the newly-resolved kind, the enforcer
calls back into `RoutingService::apply_rule_for_device(id, Direct)` to unbind the
tunnel binding. The enforcer's startup `reconcile` runs the same clamp sweep,
and `main.rs` orders `RoutingService::reconcile` **before**
`ZoneEnforcementService::reconcile` so the clamp sees the routing bindings after
they are (re)applied.

**Alternative rejected:** making `RoutingService::resolve_target` zone-aware
(clamp at the single point where `ip rule`s are created). That is fewer moving
parts and needs no event, but it pushes zone policy into the routing engine's hot
path, contradicting the #735 design that keeps the routing engine zone-free and
reads only `default_policy`. We chose the event + callback to preserve that
boundary, accepting its cost: a device's stored rule stays `Default` while its
applied binding is `Direct` (a benign divergence), and the clamp must be
re-derived on every boot rather than persisted.

## Non-goals

- **No same-subnet peer isolation.** On a flat L2 segment the daemon never sees
  peer↔peer traffic, so this layer cannot affect it — it is the AP's job, or the
  `IsolateMembers` rung (#737). This is an epic-wide non-goal, restated here so
  the packet-layer's honest limit is not mistaken for a bug.
- **No per-tunnel egress gating.** `allowed_targets` is coarse (`direct` /
  `tunnel`); the egress gate permits/forbids "any tunnel," never a specific
  tunnel UUID.

## Consequences

- **Blast radius:** `RoutingServiceImpl` gains an `EventPublisher` dependency and
  the `FirewallManager` trait gains three methods (`apply_zone_rules`,
  `remove_zone_rules`, `list_zone_rule_ips`) plus an `input` base chain — a
  wider firewall surface every implementation (netlink, no-op mock, test spies)
  must satisfy.
- **Verification:** the netlink/firewall code compiles only on Linux
  (`netlink-sys`), so the packet rules themselves are gated behind
  `make check-daemon` (container) / CI; the cross-platform enforcer decision
  logic is unit-tested natively with a recording firewall + routing spy over the
  real in-memory repositories.
- **Divergence:** a clamped device shows `Default` in the database and `Direct`
  in the applied routing state until the policy or its zone changes again. This
  is intentional (the stored rule is re-derivable intent) and is reconciled on
  every boot.
