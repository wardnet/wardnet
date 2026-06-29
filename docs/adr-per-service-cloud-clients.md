# ADR: Per-service cloud clients with independent endpoints

**Status**: Accepted
**Date**: 2026-06-29
**Issue**: #610 (cloud decomposition); pairs with
[`adr-daemon-cloud-auth.md`](adr-daemon-cloud-auth.md) and
[`adr-service-decomposition.md`](adr-service-decomposition.md)

---

## Context

wardnet-cloud is decomposed into a **global tenants** service (accounts, billing
linkage, the global naming authority — one instance, at
`account.wardnet.network`) and **per-region ddns** services (network
registration, A-record publishing, ACME DNS-01 — one instance per region, behind
region-specific FQDNs). The daemon must reach both.

A single monolithic "cloud client" pointed at one base URL no longer fits: the
two services live at different origins, scale independently, and a network is
pinned to a *specific* region's ddns endpoint after registration. The daemon
needs to address each by its own URL while still presenting one identity.

## Decision

### 1. One client type per cloud service, each with its own base URL

The daemon's `cloud/` module exposes a **`TenantsClient`** (global endpoint) and
a **`DdnsClient`** (regional endpoint), constructed independently:

- `TenantsClient` is bound to the **global** `TENANTS_BASE_URL`
  (`account.wardnet.network`). It owns enrollment, verification-code requests,
  slug availability, network registration, JWT minting, and per-daemon removal.
- `DdnsClient` is bound to a **regional** ddns control base URL resolved from a
  **hardcoded region catalog** (region slug → `ddns.svc.<...>.wardnet.network`).
  It owns A-record publishing and the ACME DNS-01 challenge.

The clients share the pooled `reqwest::Client` (connection reuse) but nothing
else; each is cheap to build per use.

### 2. One identity, threaded through both

Both clients authenticate as the same box via a single shared
[`DaemonIdentity`](adr-daemon-cloud-auth.md) (Ed25519 key + cached JWT + shared
entitlement handle). The JWT is minted from tenants (PoP) and presented as a
bearer to the regional ddns service. So the daemon has *many endpoints* but
*one credential* and *one entitlement state*.

### 3. Region is data, not a build constant

The region catalog is a hardcoded table in the daemon today (a FIXME to confirm
the production FQDNs with infra), keyed by region slug. A network's region is
chosen at registration (lowest-latency probe) and persisted in `system_config`;
the daemon resolves its ddns base URL from the catalog on each use rather than
hardcoding a single endpoint.

### 4. The cloud edge demuxes by SNI

The regional ddns services sit behind an edge that routes by **SNI** (the
client's TLS server-name), so multiple logical services can share an ingress
without path-based coupling. The daemon's only obligation is to address each
service by its correct FQDN; the edge does the rest.

## Consequences

- **Endpoints evolve independently.** Adding a region, or moving a service, is a
  catalog/URL change — no entanglement between the global and regional call
  paths.
- **Blast radius is per service.** A regional ddns outage cannot take down
  enrollment or billing-linked tenant operations, and vice versa.
- **One identity to reason about.** Despite multiple endpoints, there is a single
  `DaemonIdentity` and a single `Entitlement` flag, so auth and suspension logic
  stays centralized (see the companion auth ADR).
- **Coordination cost:** the hardcoded region catalog must track infra reality;
  a wrong FQDN is a daemon release. This is an accepted trade for not standing up
  a daemon-facing discovery service pre-GA.

## Considered options

- **One monolithic cloud client, one base URL** — rejected. It cannot address a
  global service and N regional services at distinct origins, and it couples
  unrelated call paths behind a single endpoint.
- **Per-call base URL on a single client** — rejected as a thin disguise for the
  monolith: it scatters endpoint knowledge across call sites instead of owning it
  in a typed client per service, and muddies which identity/scope each call uses.
- **Daemon-facing service discovery** — deferred. A discovery endpoint is the
  right long-term answer to the hardcoded catalog, but it is unjustified
  infrastructure pre-GA with a handful of regions.
