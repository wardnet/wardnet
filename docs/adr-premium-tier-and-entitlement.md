# ADR: Premium tier and entitlement model

**Status**: Accepted
**Date**: 2026-06-13
**Issue**: #609 (premium tier + entitlement); pairs with #610 (bridge decoupling)

---

## Context

Wardnet is pre-launch with a single real install. The remote-access plane —
DDNS, the relay, and the ACME backplane — is going live on shared cloud infra
(Hetzner VM + Neon Postgres + Cloudflare). Unlike the on-premises daemon
(which is free to run and self-contained), these wardnet-operated capabilities
**cost real money per active install**. That inverts normal SaaS economics:
the paid tier gates a *cost centre*, not just software.

We want to monetize without (a) the clawback trap of giving a capability away
free and later charging for it, and (b) standing up premature identity
infrastructure. Two principals exist today — the **install identity** (Ed25519,
device-keyed) and the **admin session** (local password). Neither is a billing
account.

## Decision

### 1. Two tiers, split on cost-bearing capability

- **Free tier** — self-host with your own domain (the BYOD-Cloudflare
  `DnsProvider`). Full features, uncapped, forever-free; touches no
  wardnet-operated service beyond release downloads, so it costs us nothing.
  This is the growth surface.
- **Premium tier** — paid. Grants the two cost-bearing capabilities: the
  **DDNS service** (a managed `<vanity>.my.wardnet.services`) and the
  **Tunneler** (private DNS while roaming). Premium buys *not needing a domain*,
  plus the tunnel — not the mobile PWAs themselves, which a free BYO-domain user
  also gets.

### 2. Durable account, ephemeral install binding

Billing is anchored to a **durable, email-keyed premium account** whose master
record lives in **tenant management** (alongside the global naming authority).
**Stripe is referenced** (`stripe_customer_id`), **not authoritative** for
identity — so the payment processor can be swapped without losing accounts.
The Ed25519 install key is an **ephemeral install binding**: one *active*
binding per subscription; a reinstall re-binds the single slot (no re-payment)
by proving account ownership via an **email magic-link**. A second simultaneous
install is a second subscription.

### 3. Entitlement derived from Stripe

**Entitlement** is derived from Stripe via webhooks plus a nightly
reconciliation: **active through `past_due`** (a failed card enters dunning
grace, not an instant cutoff) and **revoked on `canceled`**. It is held on the
tenant-management account record.

### 4. Entitlement lease as the enforcement token

Tenant management (global) signs a short-lived **entitlement lease**
`{install_id, entitled, exp}` (TTL ~7 days; the daemon refreshes daily, so a
transient outage cannot break a paying customer for up to the TTL). It is
verified **locally** by the regional DDNS and Tunneler services and by the
daemon, against tenant's public key. Enforcement is **belt-and-suspenders**:
regional services refuse at the boundary *and* the daemon self-degrades.

### 5. Suspended mode

Once a lease goes invalid the daemon enters **Suspended**: user/admin PWAs
return `403`, the Tunneler drops, and ACME renewal stops (the cert ages out
within ≤90 days). Suspended ≠ Free tier — a Suspended install has no working
domain until it resubscribes or adds its own. **Re-entry is always reachable**:
the desktop admin site during the cert window, plus a LAN-local HTTP admin
fallback after expiry; resubscribing refreshes the lease and restores service.

### 6. No OAuth/IdP server

Machine auth is the Ed25519 install key; human recovery is the magic-link;
future inter-service auth is mTLS (see the service-decomposition ADR). We do
**not** run an OAuth/identity server. This is revisited only if we adopt a
household/org account model spanning many installs and human users.

### 7. Go-to-market commitment (engineering-constraining)

DDNS and the Tunneler are **paid from public availability**. During the public
beta they are free but **explicitly labelled "free during beta → premium at GA,
with advance notice"** — never silently free, to avoid the clawback trap. At GA,
premium signups use a **Stripe-native trial**. Positioning leads with the
free/self-hostable product; premium is an honest, secondary "supports the
project" tier.

## Consequences

- The **entitlement lease** doubles as the global↔regional boundary primitive
  (see `adr-service-decomposition.md`) — one mechanism, two problems solved.
- **Tenant management is the security-critical store** (accounts, billing
  linkage, lease-signing key) and must be isolated from the internet-facing
  planes.
- We owe **transactional email** (magic-link) regardless of other choices.
- The daemon must ship a **LAN-local HTTP admin fallback** so a Suspended
  install is never a roach-motel.

## Considered options

- **Stripe as the master account record** — rejected. The bridge already needs
  an account row (it holds the install binding and subdomain), so this was never
  "one store vs two"; making Stripe authoritative only adds processor lock-in.
- **Zitadel / an OAuth server** — rejected. It would be a second account store
  to sync, runs OIDC against an origin that doesn't exist until *after* the
  rebind (chicken-and-egg on a fresh reinstall), and guards exactly one
  operation. Justified only by a future org/multi-user model we are not building.
- **Day-zero hard paywall** — rejected. No reputation to spend; the free tier is
  the growth engine and costs us nothing.
