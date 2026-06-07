# ADR: Bridge edge topology — Caddy-l4 on the front, bridge as passthrough tunnel router

**Status**: Accepted
**Date**: 2026-06-07
**Issue**: #541 (follow-up to the #521 HTTPS/DDNS + remote-access umbrella)

---

## Context

The bridge is the public, cloud-hosted entrypoint of the remote-access plane: it
demuxes inbound TLS by SNI and either serves its own control-plane API or relays
tenant traffic to a home Pi over a pre-established reverse tunnel. It also holds
the sensitive credentials of the plane — the Cloudflare DNS-edit token and the
database DSN.

The **current** topology has the bridge own `:443` (and `:853` for DNS-over-TLS)
with a hand-rolled SNI demuxer (`source/bridge/src/sni/`):

- SNI = `bridge.<region>.wardnet.network` (the bridge's own hostname) →
  `copy_bidirectional` to **Caddy** on `127.0.0.1:8443`, which terminates TLS and
  reverse-proxies back to the bridge's HTTP API on `127.0.0.1:8080`.
- SNI = `*.my.wardnet.services` (a tenant/user host) → forwarded, still encrypted,
  over the tunnel the Pi opened outbound (`TunnelRegistry` / `TunnelRouter`); TLS
  terminates on the Pi.

This means a request to the bridge's *own* hostname travels **bridge → Caddy →
bridge** — Caddy already holds that certificate, so routing it through the bridge
first is a pointless hop.

A neighbouring decision was also in the air: the Pi **daemon** recently dropped
Caddy and now terminates TLS itself via ACME DNS-01 (`rustls` + `instant-acme` +
`rcgen`). The natural question was whether the bridge should do the same. It
should not, and this ADR settles the bridge's edge topology in the *opposite*
direction.

## Decision

### 1. Caddy (with `caddy-l4`) is the single front door

Caddy owns `:443` and `:853`. Using the `caddy-l4` layer4 module it routes by TLS
SNI and **terminates only its own hostname**, passing everything else through
untouched:

```
:443  match tls sni bridge.<region>.wardnet.network → tls (terminate) → proxy 127.0.0.1:8080
      match tls sni *.my.wardnet.services           → proxy 127.0.0.1:8081   (passthrough, no terminate)
:853  match tls sni *.my.wardnet.services           → proxy (passthrough)  → bridge DoT listener
```

`caddy-l4`'s `proxy` handler supports raw TLS passthrough and per-route selective
termination, so one config covers both "terminate for me" and "relay for tenants".

### 2. The bridge stops owning the edge

The bridge no longer binds `:443`/`:853` and no longer demuxes "mine vs Caddy".
It becomes:

- the HTTP control-plane API on `127.0.0.1:8080` (unchanged, now strictly behind Caddy), and
- a plaintext **tenant-passthrough tunnel listener** on `127.0.0.1:8081` (+ a DoT
  listener) that receives still-encrypted tenant streams from Caddy and routes
  them onto the correct reverse tunnel.

The `caddy_addr` config and the demuxer's Caddy branch are removed.

### 3. Tenant tunnel selection: bridge re-parses the ClientHello (for now)

On the passthrough path the bridge still needs the SNI to pick *which* tunnel.
The minimal, low-risk choice is for the bridge to **re-parse the `ClientHello`**
on `:8081` (reusing the existing `parse_sni`). The SNI is then parsed twice (once
in Caddy to route, once in the bridge to select the tunnel), which is negligibly
cheap. The bridge keeps `parse_sni` purely for tunnel selection.

A later optimisation may have Caddy carry the SNI to the bridge via **PROXY
protocol v2** (authority TLV), after which the bridge could delete its
`ClientHello` parser entirely. This is deferred until the TLV behaviour is
verified; it is not required for the topology to work.

### 4. Why the opposite call to the daemon is correct

The daemon dropped Caddy because it runs behind home NAT — HTTP-01 is impossible,
and it must ship as a single self-contained binary. The bridge is a public cloud
VM where `:443` is reachable, Caddy is cheap to run, and — critically — keeping
public TLS termination in a separate, hardened process keeps it **out of the
address space that holds the Cloudflare token and DB credentials**. Same product,
different environment, different right answer.

## Consequences

- **`caddy-l4` is a non-standard module** → the bridge needs a custom `xcaddy`
  build (owned by the infrastructure repo), not a stock Caddy binary.
- **Caddy owns both edge ports.** `:853` (Android Private DNS / DoT) moves to the
  same layer4 SNI match; the bridge's DoT handling becomes a passthrough listener.
- **Client IP on the tenant path.** With Caddy in front, the bridge's tunnel
  listener sees Caddy's address unless PROXY protocol is enabled Caddy→bridge.
  Required if tenant-path rate-limiting / abuse logging needs the real client IP.
- **Defense-in-depth is preserved**, not reduced: TLS termination stays in Caddy,
  away from the credential-holding bridge process.
- **The bridge gets simpler**: it sheds edge ownership (`:443`/`:853` bind,
  `caddy_addr`, the demuxer's Caddy branch) and keeps only API + tunnel relay.
- **Supersedes** the "Caddy on bridge nodes" section of `source/bridge/PLAN.md`
  (which described Caddy *behind* the bridge demuxer on `:8443`).
- **Reversal trigger**: if operating a custom `xcaddy` build proves more costly
  than it's worth, the bridge can reclaim `:443` and terminate its own hostname
  in-process via DNS-01 (it already holds the Cloudflare token) — the tenant
  passthrough path is unaffected either way.
