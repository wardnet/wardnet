---
status: accepted
date: 2026-06-05
issue: "#529 (part of #521)"
---

# ADR: TLS serving identity is a method-exposed projection, and the canonical FQDN is the cert domain

---

## Context

C7 (#528) made `wardnetd` terminate TLS itself: an always-bound `:443` behind a
503 "not provisioned" guard, and a `:80` listener that 308-redirects to HTTPS.
C8 (#529) adds two things that both need to read the daemon's *current serving
identity* — the domain whose certificate is live on `:443`:

1. The `:80` redirect must rewrite short/LAN names (`wardnet`, `wardnet.lan`, the
   bare LAN IP) to `https://<canonical-FQDN>` so the client lands on the name with
   a valid certificate.
2. The split-horizon `<canonical-FQDN> → Pi LAN IP` system record must track the
   same domain.

Two design questions fell out of this.

**(a) How do the unauthenticated `:80`/`:443` listeners learn the serving
identity?** C7 threaded a raw `Arc<AtomicBool> provisioned` flag from
`build_tls_state` into both the guard and the `CertActivator` impl. Extending that
to also carry the FQDN would mean a second raw shared cell crossing the
binary↔service boundary. The alternatives were: (i) have the listeners call an
admin-gated service (`TlsService::status()`) per request, or (ii) read
`system_config` directly from the handler.

There is **no precedent in the codebase for an unauthenticated inbound request
elevating to an admin context** — every `auth_context::with_context(Admin …)` is a
trusted background runner acting on the daemon's own behalf. Doing it from a
network-facing handler would launder admin authorization onto a public endpoint.
Reading the DB directly from the handler bypasses the service/auth layer for the
same effect. And `TlsService::status()` is async + DB-backed — unfit for the
`:443` guard's per-request hot path.

**(b) What *is* the canonical FQDN?** The #529 issue text says it "comes from
`DdnsService::status().fqdn`" — the configured DDNS hostname, which is set the
moment DDNS is registered. But the acceptance criterion is "resolves … *with a
valid cert*", and the configured hostname and the cert-covered hostname diverge
in transient windows: issuance lag after registration, a provider/domain change
before re-issuance, or an ACME failure.

## Decision

**(a) The serving identity is encapsulated by a serving-layer component
(`ServingControl`) and exposed through a read-trait (`ServingIdentity` —
`is_provisioned()` / `canonical_fqdn()`).** The unauthenticated `:443` guard and
`:80` redirect depend on `Arc<dyn ServingIdentity>` and **call methods**; they
never read a raw shared cell, never call an admin-gated service, and never
elevate to an admin context. `TlsService` writes the identity through the
`CertActivator::activate(chain, key, fqdn)` seam at the moment it swaps the cert.
The authoritative copy of the served domain still lives in `system_config`
(`tls_cert_domain`, owned by `TlsService`) and is what the admin API reads via
`TlsService::status()`; `ServingControl` is the hot-path **projection** of that
truth. Provisioning collapses into the projection: a `Some(domain)` ⟺ provisioned,
so the 503 gate and the redirect target move together by construction.

**(b) The canonical FQDN is `tls_cert_domain` — the domain a certificate was
actually issued/activated for — not `DdnsService::status().fqdn`.** Reading the
single key carries both the name *and* the valid-cert guarantee; on a fresh
install it is `None`, so the redirect and split-horizon record stay inert until a
real cert exists. This intentionally contradicts the #529 issue text.

## Consequences

- The redirect/record are **self-gating**: they appear exactly when they will
  work and, on a domain change, keep pointing at the still-cert-valid domain until
  the new cert lands — never at a name that would produce a TLS error.
- There are **two representations** of the served domain: the authoritative
  `system_config` value (for the API) and the in-memory `ServingControl`
  projection (for the hot path). They are reconciled by `activate()` and seeded at
  boot from the stored `tls_cert_domain`. This is deliberate CQRS-style
  separation, not accidental duplication.
- C7's `Arc<AtomicBool> provisioned` flag is **removed** in favour of the
  method-based component, so only one pattern exists.
- A future reader comparing the code to issue #529 will find the FQDN source
  disagrees on purpose — this ADR is the reason.
