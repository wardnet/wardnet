# ADR: Daemon-owned TLS termination — native ACME, no Caddy on the Pi

**Status**: Accepted
**Date**: 2026-06-09
**Issue**: #534 (C13 docs) — records the design shipped in #528, diverging from #436

---

## Context

Issue **#436** specified HTTPS on the Pi by **bundling Caddy**: a second static
binary shipped in the release tarball, run as a **companion systemd service**, with
the daemon writing the **Caddyfile**. Caddy would terminate TLS on `:443`, obtain
the certificate via ACME **DNS-01** (the Pi is behind NAT/CGNAT, so HTTP-01 is
impossible), and reverse-proxy to `wardnetd` on `:7411`. The stated rationale was
that implementing ACME DNS-01 natively in Rust would be "months of work."

By the time C7 (#528) was built, that calculus had changed: `instant-acme` +
`rcgen` made native DNS-01 issuance tractable, and several daemon features that
landed alongside TLS — the **DnsProvider** publish abstraction, the split-horizon
LAN record, the **serving identity** / **canonical FQDN** projection, the
short-name redirect — all needed the daemon to *own* the certificate lifecycle
anyway. Driving those through an external Caddy process (managing its config file,
reading its cert state) would have been the awkward path.

## Decision

**`wardnetd` terminates TLS itself, in-process. No Caddy ships on the Pi.**

- Issuance/renewal is native: `instant-acme` runs the ACME **DNS-01** order,
  `rcgen` generates the CSR + leaf key locally, `x509-parser` reads `not_after`.
  The leaf private key **never leaves the Pi** and is stored only through the
  **SecretStore** abstraction.
- `:443` is served with `axum-server` + `rustls`, always bound: a self-signed
  **placeholder cert** seeds it pre-provisioning behind a `503` gate, swapped for
  the real cert on activation (see `adr-serving-identity-boundary.md`). `:80`
  redirects to HTTPS and rewrites short/LAN names to the **canonical FQDN**.
- A `TlsRenewalRunner` re-issues on a 12-hour tick (idle until DDNS is configured),
  renewing within 30 days of expiry.

## Consequences

- **One self-contained binary**, no companion systemd unit, no bundled Caddy in
  the release matrix, no Caddyfile generation. Operationally simpler on a Pi.
- The daemon **owns the whole cert lifecycle** — placeholder, serving identity,
  hot-swap, renewal — which is what the surrounding C7/C8 features required.
- **Diverges from #436 as written.** #436's "why Caddy" (avoid months of native
  ACME work) was reassessed: `instant-acme` made native DNS-01 small. A reader
  comparing the shipped daemon to #436 will find the Caddy companion absent on
  purpose — this ADR is the reason.
- DNS-01 challenge publishing is delegated to the **DnsProvider** abstraction
  (bridge or BYOD-Cloudflare) — see `adr-provider-based-ddns.md`.
- The **bridge** later made the *opposite-looking* call (it had used Caddy too, then
  dropped it for in-process HTTP-01 termination) for environment-specific reasons —
  see `adr-bridge-self-terminated-tls.md`. Same product, different environment.
