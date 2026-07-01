# ADR: Network Zone isolation — the guarantee ladder, coarse target gating, and a destructive device rebuild

**Status**: Accepted
**Date**: 2026-07-01
**Issue**: #735 (Phase 1 of epic #244 — Network Zones)

---

## Context

Epic #244 replaces the never-built `guest: bool` idea with a first-class
**Network Zone** model: a named policy bucket a device belongs to (exactly one)
that gates the device's allowed routing targets, its reachability of the Pi's
admin surfaces, and — in later phases — its network isolation from other zones
and from same-zone peers.

This ADR records the decisions taken in #735, which is **Phase 1: data model,
service, and API only — no packet enforcement**. Merging #735 changes no
packet's fate; it records intent and adds API-level consistency so that the
enforcement children (CI-2 #736, CI-3 #737, …) have a stable substrate to build
on. Several of these decisions are hard to reverse (a destructive migration) or
surprising (zones *gate but do not replace* routing targets), which is why they
are recorded here rather than left implicit in the code.

## Decision

### 1. The guarantee ladder, as built

Cross-zone isolation is modelled as a **`ZoneStance`** — a rung on a guarantee
ladder. Only the rungs that have backing issues exist as enum variants:

- **`SharedSubnet`** — nftables egress + admin-UI gating only; peer isolation is
  delegated to the access point. This is the honest Phase-1/CI-2 guarantee: on a
  shared L2 segment the daemon cannot prevent two devices from talking directly
  without the AP's cooperation. All three seed zones ship `SharedSubnet`.
- **`IsolateMembers`** — per-device `/32` + proxy-ARP; requires Wardnet to own
  DHCP. This is the CI-3 (#737) rung and is *recorded-only* in Phase 1.

A second, **orthogonal** axis, `member_isolation: bool`, expresses "within an
isolate-members zone, also isolate same-zone peers." It is independent of the
cross-zone rung and, like the stance, is unenforced in Phase 1.

Honest limits, stated plainly: on a shared subnet the peer-isolation guarantee
is only as strong as the AP. The ladder makes the weaker-but-deployable rung the
default and the stronger-but-demanding rung an explicit opt-in.

### 2. Zones gate, but do not replace, routing targets

`allowed_targets` is a list of coarse target **kinds** — `Direct | Tunnel` —
*not* tunnel UUIDs. A zone permits "direct" and/or "any tunnel." This keeps
zones decoupled from the tunnel catalog: no per-tunnel foreign keys, no
tunnel-delete scrubbing, no cross-entity coupling.

The zone gate runs at rule-write time in `DeviceService::set_rule` /
`set_rule_for_ip` — the single choke point for both admin and self-service
routing writes — and returns **409 Conflict** when the target's resolved kind is
not in the device's zone's `allowed_targets`. Zones therefore *constrain* the
routing choice; they do not make it. `admin_locked` remains a separate gate
governing *who* may change a rule.

**`RoutingTarget::Default` is resolve-then-check.** Validation resolves `Default`
to a concrete kind via the global default policy (read directly from
`system_config`'s `default_policy` key, mirroring `RoutingService` — a
service→service call would introduce a cycle), then checks the *resolved* kind.
`Default` is never itself an entry in `allowed_targets`.

**Known caveat:** already-stored `Default` rules are validated at write time
only. If the global policy later flips (e.g. `direct` → a tunnel) a stored
`Default` rule is not retro-validated here. Reconciling stored rules against a
changed policy belongs to the CI-2 enforcer (#736), not to this write-time gate.

### 3. Two default flags, not one

`is_default` (the **anchor**) marks the protected "home" zone — full trust,
deletion-guarded. `is_default_for_new` marks where freshly-discovered devices
land. They are distinct: the anchor is **Trusted**; the landing zone for new
devices is **Guest**. Both are enforced as at-most-one via partial unique
indexes, and both move only by *promoting* another zone (a `Some(false)` on
either flag in an update is rejected — you cannot clear a default, only relocate
it).

Membership is **sticky**: `devices.zone_id` is `NOT NULL` and is set once, from
`is_default_for_new`, at discovery-insert time. Re-pointing `is_default_for_new`
later does not move existing devices; there is no read-time resolution.

### 4. Naming: `NetworkZone` everywhere

"zone" is already overloaded — the DNS subsystem has `DnsZone` and
`ListZonesResponse` / `CreateZoneRequest` DTOs. To avoid collision and
confusion, the network-policy concept is `NetworkZone` throughout (module,
struct, DTOs, events, SDK), and its REST surface is segmented as
`/api/network/zones` (like `/api/dns/local/zones`), leaving the bare `zones`
term to DNS.

## Non-goals

- **No ARP spoofing / forced peer isolation on a shared subnet.** The daemon does
  not attempt to break same-segment peer traffic by poisoning ARP. Peer
  isolation on `SharedSubnet` is delegated to the AP; stronger isolation requires
  the `IsolateMembers` rung (Wardnet-owned DHCP + per-device `/32`).
- **No VLAN / 802.1Q.** There is no `Vlan` stance variant and no issue backing
  one. VLAN tagging is explicitly out of scope and should be revisited only
  if/when a concrete issue exists — not smuggled in as an enum variant now.
- **No packet enforcement in Phase 1.** nftables egress + admin-UI gating (CI-2
  #736), per-zone subnets / isolate-members / casting / mDNS reflector (CI-3
  #737), quarantine + notifications (CI-4 #738), UI (CI-5 #739), and E2E (CI-6
  #740) are all later children.

## Migration — destructive device-table rebuild

SQLite forbids `ALTER TABLE ADD COLUMN` of a `NOT NULL` column that also carries
a `REFERENCES` clause, so `devices` gains its `zone_id NOT NULL REFERENCES
network_zones(id) ON DELETE RESTRICT` via the established table-rebuild pattern
(cf. `20260506000000_dns_filtering.sql`).

Crucially, the rebuild **does not copy existing device rows**. Every device — and
its cascaded routing rules, DNS capture, filter assignments, and rule requests —
is cleared. On next discovery every device re-enters under the default-for-new
zone (**Guest**). The rationale: the user wants nothing auto-trusted. Backfilling
all devices to Trusted (the epic's original plan) would silently grant full trust
to every previously-seen device and would risk orphaned routing rules that
contradict a device's new zone. A clean sweep + `is_default_for_new = Guest`
achieves the "nothing auto-trusted" intent without those hazards.

**This supersedes epic #244 locked-decision #7** ("backfill all devices to
Trusted, zero behavior change"). The epic must be updated to reflect the
destructive rebuild.

## Consequences

- **Reversibility:** the destructive migration is one-way. There is no
  down-migration that restores the cleared device rows; operators upgrading past
  this point re-discover devices from scratch. This is acceptable because device
  discovery is automatic and fast, and no user-authored data other than routing
  rules is lost (routing rules are re-derivable intent, not records).
- **Consistency:** `allowed_targets` is now enforced at write time, so the system
  can no longer store a routing rule that contradicts a device's zone — closing
  the "dead data" gap where the field would otherwise be advisory.
- **Forward-compatibility:** `NetworkZoneChanged` / `DeviceZoneChanged` events are
  emitted now with no consumer, so CI-2/CI-3 enforcers can subscribe without a
  schema change. `subnet` and `member_isolation` are recorded-only, reserving the
  shape the DHCP-mode rung needs.
