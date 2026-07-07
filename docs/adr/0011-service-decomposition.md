# ADR: Bridge service decomposition — tenant (global) / DDNS + Tunneler (regional)

**Status**: Accepted
**Date**: 2026-06-13
**Issue**: #610 (bridge decoupling); pairs with #609 (premium tier). Builds on `0005-two-domain-strategy.md` and `0004-global-naming-authority.md`

---

## Context

The bridge began as a single deployment hosting three concerns —
tenant/registration, DDNS, and the tunnel relay — over coupled storage. The
coupling produced a concrete design knot that stalled work: a "regional vs
global database" question that has no clean answer while every concern shares
one store. The knot is a *data-topology* problem, not a process-boundary
problem: physically splitting services does not resolve it by itself.

This ADR settles the target decomposition and, critically, the boundary that
makes it tractable.

## Decision

### 1. Three services

- **Tenant management** — *global*, and the single global database. Owns
  premium accounts, entitlement, install bindings, the **global naming
  authority** (vanity-name allocation), Stripe linkage, and **entitlement-lease
  signing**.
- **DDNS** — *regional*. Manages DNS records, writing into the global Cloudflare
  `wardnet.services` zone.
- **Tunneler** — *regional*. The relay data plane; the daemon connects to its
  pinned-region PoP.

### 2. Why split — isolation, not scaling

The primary driver is **blast-radius isolation of the crown jewels**: tenant
management is the lowest-traffic but most security-sensitive component
(accounts, billing linkage, the lease-signing key). Keeping it off the
internet-facing, high-traffic DDNS and tunnel planes is the reason to split,
ahead of any scaling argument. This is **not** a per-concern microservice split
for throughput.

### 3. Data topology — one global DB

Only **tenant management** holds a global database. The regional services hold
only operational/ephemeral state and **never query the global DB on the hot
path**.

### 4. The entitlement lease is the only thing crossing the boundary

The **entitlement lease** (see `0010-premium-tier-and-entitlement.md`) —
`{install_id, entitled, exp}`, signed by tenant management — is verified
*locally* by the regional services against tenant's public key. This is what
dissolves the "regional vs global DB" knot: entitlement crosses the boundary as
a signed token, not a shared query.

### 5. DNS resolution stays global

Regional DDNS writes records into the **global Cloudflare zone** for
`wardnet.services`; there is **no per-install NS delegation**. The daemon pins a
region at install (lowest-latency probe) and stays there — no live re-homing.

### 6. No OAuth server for machine-to-machine auth

Future inter-service auth (tenant ↔ regional, control ↔ data plane) is
**mTLS**, not an OAuth client-credentials server.

## Consequences / status

Today this is still **one bridge deployment** (per-region install DB + the
global names DB). The three-way split is the **target**; module boundaries are
kept clean so extraction is cheap. The `my.wardnet.services` PSL boundary and
two-domain trust split (`0005-two-domain-strategy.md`) are unaffected and remain
load-bearing for per-tenant cookie isolation.

## Considered options

- **Two-way control/data split** (tenant+DDNS together vs Tunneler) — rejected.
  Tenant management deserves its own isolation as the crown jewel, and the
  subdomain coupling that argued for keeping tenant+DDNS together is thin: it
  occurs only at register/deregister (a low-frequency saga), while the
  high-frequency DDNS record updates are fully decoupled from account ops.
- **OAuth client-credentials for M2M** — rejected. Overkill for three internal
  services on a private network; mTLS is less code, no extra running service,
  and stronger.
- **Splitting the services to fix the data coupling** — insufficient on its own.
  The fix is the data-topology decision (one global DB) plus the lease boundary;
  the split merely forces the issue.
