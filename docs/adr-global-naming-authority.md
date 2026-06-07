# ADR: Global naming authority is a strongly-consistent Postgres registry, not DNS/KV

**Status**: Accepted
**Date**: 2026-06-07
**Issue**: #521 (HTTPS/DDNS + remote-access gateway). Companion to [adr-two-domain-strategy.md](adr-two-domain-strategy.md).

---

## Context

The two-domain ADR drops the region from user-facing hostnames: a user is
`<vanity>.my.wardnet.services`, not `<vanity>.my.use1.wardnet.services`. That
decision has a direct structural consequence — **vanity names become a single
flat global namespace**. With the region gone from the name, two bridges in two
regions can no longer each own a private slice of the namespace; they must
allocate from one shared pool. So we need an authority that answers two
questions correctly under concurrency, across regions:

1. **Availability** — is `alice` free? (Read; the setup wizard calls it live.)
2. **Allocation** — claim `alice` for exactly one registration, even if two
   users in two regions race for it at the same instant.

The bridge fleet is **multi-region with a per-region database** — each regional
bridge owns a Postgres holding *its* installs. There is no shared store today;
availability was previously a per-region `find_by_name` against the local DB,
which was only correct because the region was in the name.

Options considered for the global authority:

- **DNS as the registry** — read "does the record exist?" then create it.
  DNS APIs are not transactional; "create-if-absent" races, and a TTL on an
  unconfirmed reservation is awkward to model. DNS is the *resolution* layer,
  not the *allocation* layer.
- **Cloudflare Workers KV** — globally distributed key/value. But KV is
  **eventually consistent**: a write can take tens of seconds to propagate.
  That cannot satisfy "atomically reserve a name" — two concurrent
  registrations could both observe `alice` as free and both claim it
  (split-brain).
- **Cloudflare Durable Objects / D1** — strongly consistent Cloudflare-native
  primitives that *would* work, but introduce a new technology and a new
  operational surface for a problem an existing technology already solves.
- **A dedicated global service fronting the registry** — a Worker (or other
  service) that bridges call. Adds a network hop and a component for no benefit
  over letting the bridge talk to the store directly.

## Decision

The global naming authority is a **separate, strongly-consistent PostgreSQL
database** — the same technology the bridge already runs (and that the bridge
was just standardized onto). It holds a single `names` table whose **`slug`
column carries a `UNIQUE` constraint**. That constraint *is* the distributed
lock: there is no DNS existence-read and no separate lock service.

- **Each regional bridge connects to two databases**: the **global** DB (names)
  and its **regional** DB (installs). There is no Worker middleman and no
  separate "global naming service" — the bridge talks to the global store
  directly. The daemon never touches the global DB; it continues to call the
  bridge over HTTP (`GET /v1/names/{name}/available`, `POST /v1/register`).
- **Availability** = `SELECT 1 FROM names WHERE slug = $1` against the global
  DB. Authoritative — not DNS, not a cache.
- **Registration is two-phase**, executed inside the bridge's register handler:
  1. **Reserve** — `INSERT INTO names (slug, status, region, expires_at) VALUES
     ($1, 'reserved', $2, now() + ttl)`. Success = the name is ours; a unique
     violation = taken. Atomic, by the constraint.
  2. **Provision** — create the regional install row. The bridge creates **no**
     DNS record here: it is pure SNI passthrough, the wildcard
     `*.my.wardnet.services` → regional bridge IP is provisioned by infra, and
     the per-user cert is daemon-issued (`<vanity>` resolves via the wildcard,
     not a per-name record).
  3. **Confirm** — `UPDATE names SET status = 'active', expires_at = NULL`.
  - On any failure after reserve, **release**: `DELETE` the global `names` row
    **and** the regional install row (the saga spans both databases). A
    region-scoped scheduled sweep reaps expired `reserved` rows and their
    install orphans so a crashed registration never leaks a name.
- **DNS (Cloudflare) stays purely the resolution layer** (owned name → regional
  bridge IP). Registry and resolution remain separate, consistent with the
  "serving ≠ control plane" stance of the companion ADR.

The global DB is **stood up as its own instance now**, rather than starting as a
logical DB on an existing regional instance, to keep the trust/availability
boundary clean from the start.

## Consequences

- **Flat global names are possible** without putting the region back in the
  hostname — the whole point of the companion ADR is preserved.
- **No new datastore technology.** The authority reuses Postgres; the only added
  state is one table with one constraint and a periodic TTL sweep.
- **Atomicity is free and obvious.** The `UNIQUE` constraint replaces both a DNS
  existence-read and a bespoke distributed lock; correctness is a property of
  the database, not of application sequencing.
- **A second DB connection in the bridge.** Each bridge now holds a global pool
  and a regional pool; config and pooling must reflect both. The global DB is a
  cross-region dependency for *registration and availability* — but not for
  *resolution* (Cloudflare anycast) or for *serving existing users* (regional),
  so a global-DB outage stalls onboarding without taking running users down.
- **KV remains available as a future read-cache** for the availability hint if
  read volume ever demands it — but it is never the source of truth, because the
  authoritative gate is the atomic reserve.
- **Reversal**: the registry is provider-independent. If we later want a
  different strongly-consistent store, the `names` table and the two-phase
  protocol port directly; the decision that is hard to reverse is "flat global
  namespace" (from the companion ADR), not "Postgres specifically".
