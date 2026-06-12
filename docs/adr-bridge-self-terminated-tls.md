# ADR: Bridge self-terminated TLS — drop Caddy, own the edge in-process

**Status**: Accepted
**Date**: 2026-06-09
**Issue**: follow-up to #521 (HTTPS/DDNS + remote-access umbrella)
**Supersedes**: [adr-bridge-edge-topology.md](adr-bridge-edge-topology.md) (#541)

---

## Context

ADR #541 settled the bridge edge in favour of **Caddy-l4 as the single front door**:
Caddy owned `:443`/`:853`, terminated TLS for the bridge's own hostname, and
raw-passed tenant traffic through to the bridge's private listeners. It explicitly
kept TLS termination *out* of the bridge process for defense-in-depth, and recorded
a **reversal trigger**: "if operating a custom `xcaddy` build proves more costly
than it's worth, the bridge can reclaim `:443` and terminate its own hostname
in-process via ACME (it already holds the Cloudflare token)."

That trigger has fired. At the infrastructure level we decided **not** to run Caddy
in front of the bridge. The deployment now fronts the bridge with a **transparent
L4 reverse proxy (nginx with PROXY protocol v1)** that simply maps the public
privileged ports to the bridge's unprivileged localhost ports — it does **not**
terminate TLS and does **not** route by SNI. The bridge owns its own edge again.

## Decision

### 1. The bridge terminates TLS for its own FQDN in-process

The bridge issues a certificate for its own `INFORGE_DEPLOYMENT_FQDN` via **ACME
HTTP-01** (it is publicly reachable on `:80`, unlike the NAT'd daemon which must use
DNS-01). On `:8443` it peeks the TLS `ClientHello` SNI and:

- **SNI == its own FQDN** → terminate TLS locally (`rustls`) and serve the
  control-plane API over the decrypted connection;
- **any other SNI** → pass the still-encrypted stream through to the tenant's
  reverse tunnel, exactly as before.

`:8853` (DoT) is always passthrough. The control-plane API is therefore served
**only** over the terminated `:8443` path — never in plaintext. `:8080` serves only
the HTTP-01 challenge responder and `/health`.

### 2. The L4 proxy is transparent; client IP comes from PROXY protocol v1

nginx forwards `:80→8080`, `:443→8443`, `:853→8853` with `proxy_protocol` (v1).
The bridge consumes the one-line header first on every connection to recover the
**real client IP** — this is load-bearing, not cosmetic: registration is
rate-limited **per client IP** and the PoW nonce is IP-bound, so a plain L4 forward
(which would present the proxy's address) would collapse those into global limits.

### 3. Cert material lives in Postgres, sealed, and is multi-host-safe

The ACME account credentials + chain + leaf key are sealed with **AES-256-GCM**
(per-region `ENCRYPTION_KEY`) and stored in the regional `bridge_tls` row so the
cert survives restarts and is shared across hosts. Multi-host coordination is built
in from day one because it is cheap once the cert is already in the DB:

- the HTTP-01 token lives in a shared `acme_http_challenge` table, so LE's `:80`
  validation can land on any host;
- issuance is guarded by a **lease** (`bridge_tls_lease`, a conditional `UPDATE` —
  not `pg_advisory_lock`, which would pin a Neon connection across the ACME
  round-trip), so concurrent hosts never race-burn the rate limit;
- every host reloads (hot-swaps) its in-memory cert when the DB `version` overtakes
  what it serves.

## Consequences

- **Defense-in-depth tradeoff, owned explicitly.** TLS termination now runs in the
  same process that holds the Cloudflare token and DB DSN — the very thing #541
  avoided. We accept this: it is the cost of dropping Caddy, and it is bounded by
  the bridge already holding those credentials regardless.
- **No custom `xcaddy` build** to maintain — the reason the trigger fired.
- **Real client IP requires PROXY protocol v1** on every listener; a health probe
  that bypasses nginx and hits `:8080` directly must be tolerated (no header).
- **First-issuance window**: `:8443` serves a self-signed **placeholder** until the
  renewal loop issues the real cert (seconds, on a fast bootstrap cadence). Tenant
  passthrough is never blocked by cert state.
- **Out of scope (infra, not bridge code)**: HA load-balancer health checks and the
  FQDN's multi-IP A-record management. The bridge primitives are multi-host *safe*,
  not a full HA deployment.
- **Supersedes #541** in full: Caddy-l4, the `caddy_addr` config, and the bridge's
  "mine vs Caddy" demuxer branch are removed.
