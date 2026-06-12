# ADR: Two-domain strategy — trusted brand zone vs. untrusted user-content zone

**Status**: Accepted
**Date**: 2026-06-07
**Issue**: #521 (HTTPS/DDNS + remote-access gateway; builds on the #217 local-DNS umbrella and the daemon-owned TLS/DDNS plan)

---

## Context

Wardnet's core is the on-premises privacy gateway (it owns DHCP/DNS and
routes the LAN's outbound traffic privately). On top of that core we are
building a **remote-access plane**: a hosted DDNS + relay + ACME/TLS backplane
that lets a home box be reached from outside, and — layered on it — an
**authenticated access gateway** (a forward-auth reverse proxy) that exposes
individual LAN services to remote users without granting full L3 VPN access.

That plane needs a public naming scheme. The questions this ADR settles:

1. How many registrable domains do we operate, and where is the line drawn?
2. How are user-facing hostnames structured (region in the name or not)?
3. How are services addressed — subdomain-per-service or path-based?
4. How are TLS certificates issued for those hostnames?
5. Where do authentication cookies live, and how are users isolated from
   one another?

The starting point used a single domain with the region baked into the
user-facing host: `<id>.my.use1.wardnet.network` (where `use1` = us-east-1).

`wardnet.com` is **not available** (already registered by a third party), so
the flagship choice is between the domains we can hold.

## Decision

### 1. Two domains, split on a trust boundary

The only split the security model *requires* is **trusted vs. untrusted
content**. Marketing, the control plane, and infrastructure are all trusted
(we control every byte; infra hostnames serve no untrusted content and set
no cookies). Per-user gateway hostnames are effectively **user-controlled
content** and must not share a registrable domain with our authentication
cookies.

| Domain | Holds | Trust |
|---|---|---|
| **`wardnet.network`** | flagship brand + marketing site + control-plane auth/admin/API + all infrastructure names | trusted |
| **`wardnet.services`** | user DDNS hosts and per-service gateway hostnames, under the `my.` subtree | **untrusted** (`my.wardnet.services` is Public-Suffix-List listed); the apex stays trusted |

The PSL boundary is placed at **`my.wardnet.services`**, not the apex, so the
`wardnet.services` apex itself remains usable for trusted/cookie-bearing hosts
(e.g. a global gateway portal or status page).

`wardnet.network` is the flagship. With `.com` unavailable, `.network` is the
strongest *stable* option (no ccTLD/geopolitical tail risk) and is on-message
for a network product. We deliberately do **not** introduce `.io` as a brand
domain — it adds registry/geopolitical tail risk (the BIOT/Chagos situation)
and brand fragmentation for no security benefit. A third domain is a branding
choice, not a requirement, and we are not making it.

### 2. Region is dropped from user-facing hostnames

User-facing host: **`<id>.my.wardnet.services`** (no region label; the `my.`
label is retained). The region moves out of the *name* and into the record's
*value* and into operational/infra names only — e.g. the user record resolves
to a region-specific bridge IP whose operational name is
`bridge.prod.use1.wardnet.network`.

This **decouples user identity from region placement**: a user can be
migrated between regions by repointing their record, with no change to their
hostname, bookmarks, or TLS certificate. Region remains visible where it
*should* be (infra names), and hidden where it shouldn't (user identity).

Resolution stays **per-user deterministic** (the control plane writes
`<id>.my.wardnet.services` → that user's bridge IP). This is explicitly **not**
GeoDNS/latency routing — users are pinned to their assigned bridge, not to
the nearest resolver.

### 3. Services are addressed by subdomain, not path

Per-service hostnames: **`<service>.<id>.my.wardnet.services`**
(e.g. `jellyfin.<id>.my.wardnet.services`). We reject path-based routing
(`<id>.my.wardnet.services/jellyfin`) despite it needing only one ordinary
certificate, because most self-hosted apps assume they live at the root
(`/`): absolute links, cookie scope, redirects, asset paths, and websocket
upgrades break under a path prefix, and many apps have no reliable
"base path" support. Since the gateway's purpose is exposing *arbitrary*
third-party apps we do not control, subdomains (each app at its own root)
are the robust choice.

### 4. TLS: per-user wildcard via DNS-01

Per-user wildcard certificate **`*.<id>.my.wardnet.services`**, issued from
Let's Encrypt via the **DNS-01** challenge. Wildcards are free from Let's
Encrypt; the only requirement is programmatic DNS control, which we already
have (the same DNS-provider integration that drives DDNS). One cert per user
covers every service, and because the region is not in the name, the cert's
SAN is **stable across region migrations**. New services need no new cert.

The cert is issued with **two SANs** — the apex `<id>.my.wardnet.services`
(which serves the PWA and web admin site) and the wildcard
`*.<id>.my.wardnet.services` (per-service gateway hostnames). Because both the
apex and the wildcard authorize via the same `_acme-challenge.<id>.my.wardnet.services`
TXT name, their two DNS-01 values must be published simultaneously.

The PSL boundary at `my.wardnet.services` is a **hard prerequisite** for this:
Let's Encrypt computes its "certificates per registered domain per week"
rate limit using the Public Suffix List. With `my.wardnet.services` listed,
each `<id>.my.wardnet.services` is a distinct registered domain with its own
budget, so per-user wildcards scale. Without the listing, every user's cert
counts against a single shared budget for `wardnet.services` and onboarding
caps out after a few dozen users. See Consequences for the launch-sequencing
implication.

### 5. Cookies and per-user isolation

Two separate auth systems with separate cookie domains:

- **Control-plane auth** (account/admin) → cookies on `wardnet.network`.
- **Gateway SSO** (forward-auth in front of user services) → cookies scoped
  to **`<id>.my.wardnet.services`**.

**`my.wardnet.services`** is submitted to the **Public Suffix List**, which
makes `<id>.my.wardnet.services` a registrable boundary in the browser. A
user's SSO cookie then covers *all of their own* services
(`jellyfin.<id>…`, `grafana.<id>…`) but the browser physically prevents a
cookie that spans *across* users. This browser-enforced isolation is the
concrete reason the user-content subtree must be separate from the trusted
domain.

## DNS provider

Authoritative DNS for both zones runs on **Cloudflare** (anycast, globally
replicated), which satisfies the "no single-region SPOF" requirement for the
serving layer. Operating constraints recorded for implementation:

- **Serving ≠ control plane.** Cloudflare serving is robust; the component
  that *writes* records (DDNS + DNS-01 via the Cloudflare API) is our own
  reliability concern. If it is down, existing records keep resolving — only
  updates/issuance stall.
- **API rate limits** apply to per-user DDNS + per-user DNS-01 churn; batch
  and back off.
- **User/gateway records must be DNS-only (grey cloud), never proxied.**
  Orange-clouding would terminate TLS at Cloudflare, breaking end-to-end TLS
  to the home box and contradicting wardnet's privacy promise. Proxying is
  acceptable only for the marketing site.

## Consequences

- **PSL listing is a launch dependency.** Submitting `my.wardnet.services` to
  the Public Suffix List gates both per-user cookie isolation *and* the
  per-user Let's Encrypt rate-limit budget. PSL acceptance and propagation to
  consumers (browsers, Let's Encrypt) is slow (weeks), so the submission must
  happen well ahead of onboarding users at volume.
- **`<id>` must be globally unique.** With the region removed from the name,
  ids are no longer region-scoped; id allocation must guarantee global
  uniqueness across a multi-region, per-region-database bridge fleet. The
  mechanism is settled in the companion ADR
  [adr-global-naming-authority.md](adr-global-naming-authority.md).
- **No per-region DNS sub-zone delegation.** A flat user zone means one
  global authoritative zone; we rely on Cloudflare's anycast rather than
  regional delegation. Acceptable given the provider choice.
- **Region migrations are cheap** — repoint the record; hostname, bookmarks,
  and wildcard cert are untouched.
- **Defensive registrations are deferred.** We are not buying `.io` (or
  others) now; revisit only if the name becomes popular enough to be worth
  impersonating, and re-check `.com` for a future drop.
- **Reversal trigger**: if the Cloudflare centralization becomes a
  reputational or operational problem for a privacy brand, the authoritative
  layer can move to another anycast provider without changing the domain
  structure — the two-domain boundary and hostname scheme are
  provider-independent.
