---
status: accepted
date: 2026-08-10
issue: "#1181 (devices table has no retention policy)"
---

# ADR: `managed` becomes an explicit latching column, and only unmanaged devices are ever pruned

---

## Context

The `devices` table has no retention policy. A row is created on discovery and
never deleted. On a live box with 23 days of uptime: 42 devices, 19 named, 23
unnamed, arriving at roughly one a day and never leaving.

MAC randomization sharpens this into a real problem rather than a slow leak. A
phone that re-randomizes its address on each join mints a **new** row every
time — so the rows that will never return under the same MAC are precisely the
ones accumulating fastest. Left alone the table grows without bound, and the
device list becomes a graveyard the admin has to read past.

The obvious fix — delete devices not seen for N days — is where it stops being
simple. Something has to distinguish "a guest's phone that visited once" from
"the TV I configured six months ago and haven't power-cycled since". Today the
UI answers that with `name != null` (`Devices.tsx`, `DeviceDetail.tsx`), and
that inference is **wrong in both directions**:

- Granting a device Private DNS, pinning a DHCP reservation, or issuing it a
  Remote peer credential leaves it rendered as "Discovered" — it is configured,
  extensively, and the UI says it is not.
- A bare name, typed once and never followed by any actual configuration, makes
  a device "Managed".

Pruning on `name IS NULL` therefore does not merely mislabel things. It
**silently revokes remote access**: a roaming phone with a Private-DNS grant is
exactly the device most likely to be off the LAN for 30 days, and deleting its
row cascades the grant away. It would also orphan admin-authored MAC-keyed rows
that have no foreign key to clean them up.

So retention could not be built until the modelling bug underneath it was
fixed.

## Decision

### 1. `managed` is a stored, explicit, latching column — not a derived one

`devices.managed INTEGER NOT NULL DEFAULT 0`, promoted by any **admin**
configuration act and cleared only by an explicit release.

It cannot be derived. Any derivation is a disjunction over "does this device
have any admin artefact", which means the answer changes the moment an artefact
is deleted — so clearing a device's name, or removing one of its two DNS-filter
profiles, would silently demote it and re-expose it to the prune. Latching is
the point: a device an admin has taken responsibility for stays that way until
the admin says otherwise.

This buys the invariant the whole feature rests on:

> **`managed = 0` implies no admin artefacts exist for this device.**

Which is why the prune can delete an unmanaged row without checking anything
else, and can never orphan or revoke anything.

### 2. Only ADMIN acts promote; self-service acts deliberately do not

Promoting: naming, admin-locking, an admin-set routing rule, a routing profile
assignment, DNS-filter settings or profiles, enabling DNS capture, a
Private-DNS grant, a Remote peer credential, a DHCP reservation, a zone
exception naming the device, an explicit zone reassignment.

Not promoting: a self-service routing rule (`created_by = 'user'`), a device
rule request, a push subscription, the zone assigned at discovery, quarantine,
and every machine-derived identification signal.

A guest tapping a toggle in the user PWA is **the device asking, not the admin
deciding**. If self-service promoted, every guest phone would become
permanently exempt — and guest phones are the exact population the issue is
about. The gate is the auth context at the call site, not the stored row:
`update_device` and `set_rule` are both reachable by an
`AuthContext::Device` and promote only under `AuthContext::Admin`.

### 3. Demotion is an explicit "Stop managing" release that reverts everything

`POST /api/devices/{id}/release` reverts every managed setting to default and
*then* sets `managed = 0`, in that order. It is destructive and confirmed: it
revokes the device's Private-DNS grant and its Remote peer credential,
disconnecting it.

Reverting **everything** — including the two credentials — is the part worth
stating. A release that left a credential in place would hand back an unmanaged
device still holding live remote access, which the prune would then delete 30
days later with nothing but a log line. Partial release is how you build the
bug this ADR exists to remove.

`managed = 0` being **last** makes a partial failure safe: the device is left
still managed, never half-released, and every step is idempotent so a retry
completes.

### 4. The TTL is hard-coded at 30 days

`DEVICE_RETENTION_DAYS: i64 = 30`, matching the `INTRADAY_RETENTION` /
`NOTIFICATION_RETENTION_CAP` precedent.

**This deviates from the issue**, which asks for a configurable TTL. Deliberate:
a setting needs a UI, a migration, a validation story and a support answer for
"why did my device vanish", and none of that is worth shipping before we know
anyone wants a number other than 30. Revisitable without a data migration —
the column and the prune predicate do not change.

## Rejected

### Testing the device's zone in the prune predicate

Tempting: "a device still sitting in the default-for-new zone was never touched
by an admin, so it is prunable." Rejected, and it is the sharpest trap here.

Zone membership is **sticky** (`CONTEXT.md`) — set once at discovery from the
default-for-new flag and never re-resolved. So `zone_id = <default-for-new>` is
not "this device was never moved"; it is "this device was discovered while that
flag pointed here". The instant an admin promotes a different zone to
default-for-new, every existing device's `zone_id` stops matching and the
predicate matches nothing. Retention would silently switch itself off — no
error, no log line, just a table that quietly resumes growing.

### Backfilling `managed` from a past zone reassignment

The migration backfills from every admin artefact **except** zone membership,
for the mirror image of the reason above: `zone_id != <default-for-new>` cannot
distinguish "an admin moved this device" from "the default-for-new flag was
later re-pointed at a different zone".

Guessing inclusively would mark essentially every existing device managed on
any box whose default-for-new flag has ever moved — disabling retention on
exactly the boxes that need it, and doing so invisibly. Guessing exclusively
would be a lie in the other direction, but a harmless one: a device that really
was moved by an admin, and has no other artefact and no name, stays unmanaged
until the next admin act. Going forward `assign_device` promotes explicitly.
This is an accepted, bounded gap in one-shot backfill accuracy, taken in
exchange for the feature actually working.

### A `CREATE TRIGGER` to promote automatically

No trigger exists anywhere in the migrations, and promotion is not a
row-shape fact — it depends on the **auth context** of the caller, which SQL
cannot see. A trigger on `routing_rules` could not tell an admin-set rule from
a self-service one. Promotion is explicit code at each call site.

### Putting the release orchestration in `DeviceService`

Forced out by an `Arc` cycle, not by taste. `InboundWgServiceImpl` and
`PrivateDnsServiceImpl` both hold `Arc<dyn DeviceService>` — they call
`mark_managed` — so a `DeviceService` that reached back into them to revoke a
peer or a grant would be a construction-order deadlock.

The release therefore lives in the API handler (`api/devices.rs`), which
already orchestrates several services and is the established home for exactly
this shape. Promotion still goes through `DeviceService::mark_managed` rather
than scattered `DeviceRepository` writes, per the single-service-per-repository
rule.

## Consequences

- **Unmanaged devices absent over 30 days are deleted**, hourly-ticked and
  once per calendar day, by a new `DeviceRetentionRunner`. Kept separate from
  `DbMaintenanceRunner`, which is database-level (vacuum, checkpoint, optimize)
  and takes only a `MaintenanceService`; this is domain policy over one table.

- **The prune must evict the pruned MAC from the discovery service's in-memory
  maps, and must delete the row first.** This is not bookkeeping. A pruned MAC
  left in `state` takes the `gone` arm on its next observation →
  `ObsAction::Reappear` with a now-dangling `device_id` →
  `update_last_seen_and_ip` matches zero rows and returns `Ok(())` **silently**
  → `handle_unknown_mac` is never reached, so the device is never re-inserted.
  The result is a device invisible in the UI while its traffic flows
  unattributed, until the daemon restarts. Evicting *before* deleting is wrong
  in the other direction: an observation in the window finds the row still
  present and re-populates `state`. Delete, then evict, holding
  `lock_for_mac(mac)` across both. This also bounds `ip_history` and
  `device_locks`, which nothing pruned before.

- **No event is published on prune.** No subscriber exists; the device departed
  at least 30 days ago and every listener tore down long before.

- **Rows deliberately left dangling.** `push_subscriptions` — by existing
  design, stated in its own migration: a forgotten-then-rediscovered device
  keeps its MAC and its subscription stays valid, and stale rows are pruned on
  404/410 from the push service. `dns_query_log` self-expires within the same
  window. `stats_*` and `notifications` are historical truth and are kept.
  `dhcp_leases.device_id` is already `ON DELETE SET NULL`.

- **The release *deletes* the routing rule rather than setting it to
  `Direct`.** Two reasons, and the second is a hard blocker. "No rule" is the
  state a never-configured device is in — it follows the gateway's global
  default policy — whereas a `Direct` rule is an explicit persisted choice that
  *overrides* that default, so writing one is not a revert. And `set_rule` is
  validated against the device's zone allow-list, so on a tunnel-only zone
  `Direct` is rejected outright and the device could never be released at all.
  Deleting cannot conflict with a zone. The new `DeviceService::clear_rule`
  publishes `RoutingRuleChanged` carrying the *global default policy* as the
  target — the routing listener applies whatever the event says, so publishing
  the deleted rule would re-install it.

- **`routing_rules.created_by` is not a usable promotion signal today.**
  `upsert_user_rule` hard-codes `'user'` regardless of caller, so an admin-set
  rule is indistinguishable in the row. The migration's `created_by = 'admin'`
  clause is the correct predicate and matches nothing at present; it is kept
  for the day rules record their true author. Promotion keys off the auth
  context instead.

- **An empty device name now clears the name to `NULL`** rather than storing
  `""`. An empty-string name renders as no label while still counting as named
  — precisely the ambiguity this ADR removes — and the release needs a way to
  actually unname a device.

- **"Managed" and "named" are now separate everywhere.** Two places that
  conflated them were corrected rather than left: the inbound-WireGuard
  `add_peer` gate and the Private-DNS grant picker both still require a *name*
  (a credential needs a human-readable label) but no longer describe that as a
  managed-state requirement — which would now be circular, since granting is
  itself one of the acts that makes a device managed.
