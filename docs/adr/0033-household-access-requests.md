---
status: accepted
date: 2026-08-14
issue: "#919 (Private DNS: user request + admin approve flow)"
---

# ADR: One access-request inbox, with per-kind approvers

*Companion to [0029-private-dns-dot.md](0029-private-dns-dot.md).*

---

## Context

Private DNS shipped **admin-grant only**. ADR-0029 §5 recorded the gap in as
many words: "v1 has no user-initiated request flow (that is #919)". A household
member who opened the user PWA's Private DNS card and had no grant reached a
dead end — a sentence telling them to go ask their administrator out of band.

The box already had a request→approve loop. `device_rule_requests` and the
`/rule-requests` admin page were built for one question: block or allow a
domain. Private DNS needs the same loop with a different subject and, crucially,
a different consequence — approving it has to *do* something, where approving a
rule request only records a decision.

So the choice was whether to stand up a second inbox next to the first, or make
the first one general.

## Decision

### 1. One inbox, called **access requests**

`device_rule_requests` becomes `device_access_requests`, with a `kind`
discriminator (`block` | `allow` | `private_dns`) and a `domain` that is only
meaningful for the kinds that name one. A second parallel inbox would have meant
two tables, two admin pages, two push notification kinds, and an admin who has to
know which surface a given ask landed on.

The name is deliberate. A bare `requests` resource — `/api/requests` — names no
domain concept and collides with "HTTP request" at every call site. **Access
request** is the term the industry converged on (Okta, Entra, GitHub, Jira all
ship one). It bends slightly for `block`, where a member is asking to *restrict*
themselves rather than to reach something; that was accepted as the cost of one
honest noun over three awkward ones.

Rejected: **per-feature paths over a shared table** (`/api/rule-requests` plus
`/api/private-dns/requests`). More RESTful in isolation, and it would have
avoided an API break — but the single admin inbox is the point, and building its
status tabs and counts from two queries merged client-side is a worse seam than
one collection. Rejected: **`approvals`**, which misnames a collection whose rows
are mostly pending or declined, and reads backwards when a member creates one.

### 2. Approval dispatches through a per-kind approver registry

Approval is not uniform, so it is not a field on the service — it is a strategy
per kind, resolved through a registry, the same shape as `AnomalyDetector`:

```rust
trait AccessRequestApprover {
    fn kind(&self) -> AccessRequestKind;
    async fn approve(&self, req: &DeviceAccessRequest,
                     params: Option<&ApprovalParams>) -> Result<(), AppError>;
}
```

**A kind with no registered approver is record-only.** That is not a hole, it is
the exact current behaviour of `allow` / `block`, preserved with no
special-casing: the admin still applies the DNS filter rule by hand. `PrivateDns`
is the only registered approver in this delivery, and it mints the grant.

The alternative — an `if kind == private_dns` inside `decide` — was rejected
because it puts every future kind's side effects into one growing `match`, and
because the admin UI needs the same seam: each kind declares what it renders and
what the admin must choose before approving. `ApprovalParams` is a typed,
per-kind payload on the decision body rather than a bag of optional fields, so a
kind that needs an admin decision can demand one.

Two ordering rules make it safe. The approver runs **before** the decision is
recorded, so a failed grant leaves the request `pending` rather than reading
"approved" against something that never happened. And a request that is already
decided is refused, so a second approval cannot re-run the side effect or rewrite
the audit trail.

### 3. Reconciliation goes over the event bus, because the other direction is a cycle

Approving a Private-DNS request calls into `PrivateDnsService`. So
`PrivateDnsService` must not call back into `AccessRequestService` — and it would
need to, because an admin can also grant a device straight from the Remote Access
card, which should not leave a phantom `pending` row nagging them about something
they already did.

`grant_device` therefore publishes `PrivateDnsGrantCreated`, and a listener in
the `wardnetd` binary resolves the pending request. The service graph stays
one-way. This is the same reasoning as `DnsDeviceSnapshotListener`: the grant is
persisted before its event is published, which is what makes the bus a race-free
choke point. The resolve is guarded on `status = 'pending'`, so the approval
path's own write and the listener's write cannot double-apply — whichever lands
second is a no-op, and the original `decided_by` survives.

The event carries the acting admin, read from the ambient auth context, so a
request resolved this way still records who decided it rather than being
attributed to the daemon.

### 4. A member may only ask where the admin can act

The Request button appears when Private DNS is enabled network-wide and this
device has no grant. It does **not** appear when the feature is off: `grant_device`
requires `enabled`, so such a request would sit in the inbox un-approvable until
the admin turned on a Premium feature. The daemon enforces the same rule, so the
UI is a convenience rather than the guarantee.

A decline is visible and re-requestable — the card says so and offers the button
again. Deliberately no push on a decline: it spends a notification on a negative
outcome, and it breaks the existing rule that decisions are pull-only.
`useMyAccessRequests` overrides the PWA's global `refetchOnWindowFocus: false`
for the same reason `usePrivateDnsMe` does — the decision is made in the admin's
browser, so foregrounding the app is the moment the answer can have changed.

**Approval needs no new client machinery at all**, but it does need one server
call: after minting the grant, `PrivateDnsApprover` fires the existing
`private_dns_granted` push itself. Granting from the Remote Access card
deliberately leaves that push to the admin's "Send to device" button, because
there the admin chooses the moment; an approval *is* that moment, since someone
asked and is waiting. The push is best-effort — the grant is already persisted,
and a member with no subscription is a `false`, not an error — so a delivery
problem must never fail an approval and leave the request `pending` against a
live grant. Either way `usePrivateDnsMe` refetches on focus, so the card reaches
the setup steps with or without the notification.

At most one *open* Private-DNS request per device, enforced by a partial unique
index rather than by a service remembering it already asked — the same reasoning
as the anomalies index. The index is scoped to `kind = 'private_dns'` on purpose:
a Private-DNS request carries no payload, so a second open one is pure duplicate
nagging, whereas a device legitimately has several `allow` requests pending at
once, one per domain.

## Consequences

- **The API rename is a break.** `/api/rule-requests` and
  `/api/devices/me/rule-requests` are gone. The JS SDK is 0.x, the Go SDK never
  had a hand-written wrapper for this resource, and every other consumer is
  in-repo, so the break was judged affordable — and cheaper than living with a
  table and a route whose names describe a third of their contents.
- **Requesting must never promote a device to *managed*.** CONTEXT.md's rule is
  that a device's own self-service acts never promote it — that is the device
  asking, not the admin deciding. Only the approval's `grant_device` promotes.
  Asserted by a test, because it is exactly the kind of invariant a future
  `mark_managed` call would quietly break.
- **`PrivateDnsMeResponse` deliberately does not carry request state.** Adding it
  would recreate the cycle decision 3 exists to avoid, so the PWA card composes
  two queries instead. One extra request on a phone, in exchange for an acyclic
  service graph.
- **Rule auto-apply is now one registration away, and is its own issue (#1197).** The
  original intent for #919 was to close the record-only gap too — "the admin only
  approves". Building it surfaced two facts that make it a design problem rather
  than a mechanical one, and both are invisible in the code:

  **Profiles combine by rank, not order.** `DeviceFilterContext::check` iterates
  *all* of a device's profiles and keeps the highest-ranked match. `Allow`(2)
  beats `Block`(1) regardless of position — so adding a second profile carrying
  an allowlist entry does override a plain block, but not because it is "on top",
  and it will *not* override an `ImportantBlock`(3).

  **Assigning any explicit profile drops the household defaults.**
  `materialise_context` treats an empty `profile_ids` as "use the default
  profiles" and a non-empty list as the complete set. Since empty is the default
  state, giving a defaults-following device an exception profile silently
  unfilters everything the defaults were blocking, unless the write materialises
  them — which then pins that device to a snapshot and detaches it from future
  changes to the defaults.

  So approving an `allow` has to decide *which profile* the rule lands on, and
  that choice has household-wide blast radius. That belongs in an issue with its
  own interview, not smuggled into a Private DNS one.
- **Reversibility is good.** Everything is additive behind the rename: a `kind`
  column, a registry with one entry, one new event, one listener. Registering a
  second approver is a constructor argument.
