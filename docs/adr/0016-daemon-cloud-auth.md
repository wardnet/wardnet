# ADR: Daemon cloud auth — tenants-minted JWT + Ed25519 PoP, entitlement via token-mint 403

**Status**: Accepted
**Date**: 2026-06-29
**Issue**: #610 (cloud decomposition); supersedes decision #4 of
[`0010-premium-tier-and-entitlement.md`](0010-premium-tier-and-entitlement.md)

---

## Context

The daemon talks to a decomposed wardnet-cloud: a global **tenants** service
(account + naming authority, reached via the global gateway under
`/tenants/…`) and per-region
**ddns** services. The daemon needs (a) a way to authenticate every cloud call
as *this* enrolled box, and (b) a signal for whether the box's subscription is
active, so it can self-degrade into Suspended mode.

The earlier design (decision #4 of the premium-tier ADR) proposed a
**short-lived entitlement lease** `{install_id, entitled, exp}` signed by the
global tenant service and verified locally by the daemon and regional services
against tenant's public key. That presumes the daemon holds and validates a
signed entitlement artifact on a refresh cadence.

In building the mesh client we found the lease to be redundant machinery: the
cloud already enforces the subscription at the point of every privileged call,
and the daemon already has to mint a credential to make those calls. The mint
*is* the entitlement check.

## Decision

### 1. The daemon's cloud credential is a tenants-minted JWT

The daemon holds a per-box **Ed25519 identity** (a 32-byte seed in the daemon
`SecretStore`, generated at enrollment). To call any cloud service it first
**mints a short-lived JWT** from the tenants service. The JWT is **opaque to
the daemon** — it does not verify the signature or inspect claims beyond reading
`exp` to schedule a refresh. The cloud is authoritative; the daemon is a bearer.

The JWT is cached in memory and re-minted on absence or near-expiry (a refresh
skew before `exp`). A malformed/missing/past `exp` is floored to a fallback TTL
so a bad token can never drive a re-mint-on-every-call loop against the mint
endpoint.

### 2. Token minting is authenticated by Ed25519 proof-of-possession (PoP)

The mint request is signed with the daemon's Ed25519 key (PoP), so the tenants
service binds the minted JWT to the holder of the enrolled key without the
daemon ever sending a long-lived secret over the wire. PoP-only auth is used on
the mint path specifically (it must not re-enter the JWT path — there is no
chicken-and-egg).

### 3. Entitlement is the token-mint outcome — no lease

There is **no signed entitlement lease**. Entitlement is derived directly from
the mint result:

- a **`403`** ("subscription is not active") on mint ⇒ the box is **suspended**;
- the **next successful mint** ⇒ the box is **restored**.

A single process-wide, lock-free `Entitlement` handle (an `AtomicBool` behind an
`Arc`) records this. The cloud clients **flip** it on every mint; the serving
layer and background runners **read** it. Edges are logged once, not every poll.

### 4. Enforcement is local self-degradation, gated on one shared flag

When suspended the daemon:

- **serving layer** — blocks the two premium app surfaces (user PWA `/` and
  admin mobile app `/admin-app/`) with a `403` "subscription paused" page, while
  leaving the admin **website** (`/admin/`) and the whole `/api/*` surface
  reachable so the operator can always resubscribe. The block is enforced on
  every listener, including the plain-HTTP `:7411` LAN admin surface;
- **DDNS runner** — stops publishing (it would `403` anyway) but keeps a cheap
  per-tick token-mint **re-probe**, so the box self-heals the moment the
  operator resubscribes, with no operator action;
- **TLS renewal runner** — goes fully inert; the public cert ages out, after
  which the LAN-local plain-HTTP `:7411` admin surface is the re-entry path.

The cloud still enforces independently at every privileged call (the mint, and
each regional operation behind it), so this is belt-and-suspenders, not the only
gate.

## Consequences

- **One mechanism, not two.** The credential the daemon must mint to do *any*
  cloud work doubles as the entitlement signal. No lease artifact, no
  lease-signing key on the global service, no daemon-side lease verification, no
  daily lease-refresh cadence distinct from the token cache.
- **Suspended is never a roach-motel.** The admin website + `/api/*` stay up on
  every listener, and the DDNS re-probe restores automatically — the daemon does
  not need the operator to click anything inside the box after resubscribing.
- **The JWT being opaque** keeps the daemon decoupled from the cloud's claim
  schema; the cloud can evolve scopes without a daemon release.
- **What is dropped:** the entitlement lease, its TTL/refresh story, and local
  signature verification of an entitlement artifact (decision #4 of the
  premium-tier ADR). The *intent* of that decision — local self-degradation with
  guaranteed re-entry — is preserved; only the transport (lease vs mint-403) is
  replaced.

## Considered options

- **Keep the signed entitlement lease** — rejected. It is a second artifact and
  signing key to operate and verify, refreshed on its own cadence, encoding
  exactly the bit the mint already returns. The mint cannot be skipped (every
  privileged call needs the JWT), so the lease is pure redundancy.
- **Long-lived JWT, no PoP** — rejected. A long-lived bearer in the daemon is a
  theft target and cannot be revoked without a denylist; PoP-minted short-lived
  tokens are cheaply rotated and bound to the enrolled key.
- **Daemon verifies the JWT signature/claims** — rejected as needless coupling.
  The cloud verifies on every call; the daemon only needs `exp` for scheduling.
