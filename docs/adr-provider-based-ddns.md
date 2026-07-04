# ADR: Provider-based, daemon-owned DDNS + ACME

**Status**: Accepted
**Date**: 2026-06-09
**Issue**: #534 (C13 docs) — records the design shipped in #527/#528 (#521 umbrella)

---

## Context

For the daemon to terminate TLS (see `adr-daemon-owned-tls.md`) it needs a public
FQDN and a way to publish the DNS-01 `_acme-challenge` TXT record. Two user
populations exist:

- **No domain / no DNS skills** — the common case. They should get a working
  HTTPS hostname with zero DNS knowledge.
- **Bring-your-own-domain power users** — they already run a domain on Cloudflare
  and want the gateway under it.

A single hard-coded path (e.g. "always call the wardnet service") would exclude the
second group; baking Cloudflare credentials into every daemon would be unacceptable
for the first. And because the service that proxies DNS for the first group is
**region-specific** (a per-region bridge VM), the daemon must also choose *which*
region to talk to.

## Decision

**A `DnsProvider` trait abstracts the publish side of DDNS, with the daemon owning
the certificate lifecycle and the key never leaving the Pi under either provider.**

- **`DnsProvider`** (bound at construction to one target): `upsert_a(ip)` publishes
  the A record; `set_txt(values)` / `delete_txt()` publish **one or more**
  `_acme-challenge` TXT values at the one challenge name (multi-valued because a
  **per-user wildcard certificate** authorizes apex + wildcard SANs through the same
  name — #540); `teardown()` removes the published presence.
- Two implementations:
  - **`BridgeProvider`** (default) — talks to a wardnet **bridge**, keyed by install
    id, every request **Ed25519-signed**. The bridge holds the Cloudflare token so
    the user needs no domain or credentials; it is a *credential proxy*, not the
    cert owner.
  - **`CloudflareProvider`** (BYOD) — talks to the user's own Cloudflare zone
    directly with their token.
- **Multi-region via a daemon-side catalog.** A built-in region-slug → bridge
  endpoint catalog ships in the daemon (`ddns/region.rs`); at registration the
  daemon probes each region's `/ddns/v1/health` and picks the lowest-latency one. The
  bridge cannot supply this list (it is itself region-bound), so the catalog is the
  daemon's. Adding a region is a one-line catalog change.
- The **daemon**, not the bridge, owns issuance and renewal — it calls the provider
  only to *publish*. The cert/signing key is generated on the Pi and never leaves.

## Consequences

- One TLS lifecycle (`TlsService` + `TlsRenewalRunner`) serves both provider kinds;
  swapping bridge ↔ BYOD-Cloudflare is a provider construction detail.
- The challenge path is **multi-valued** end to end (`set_txt(&[String])`) to carry
  the apex + wildcard challenge values simultaneously (#540).
- The region catalog is **authoritative for the bridge endpoint URL**, and that URL
  **must equal the bridge's own served FQDN** — the bridge issues its cert for, and
  matches its terminate-SNI against, exactly that hostname. A mismatch breaks both
  the daemon→bridge TLS and the bridge's own API routing (the alignment that keeps
  `region.rs`'s `base_url` equal to the bridge's `INFORGE_DEPLOYMENT_FQDN`).
- The bridge stays a **credential proxy**: it never holds the Pi's cert or signing
  key, only the Cloudflare token it proxies on the Pi's behalf.
